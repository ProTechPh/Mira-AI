use crate::modules::oauth;
use std::sync::{Mutex, OnceLock};
use tauri::Url;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio::sync::watch;
use tokio::time::{timeout, Duration};

struct OAuthFlowState {
    auth_url: String,
    redirect_uri: String,
    cancel_tx: watch::Sender<bool>,
    code_rx: Option<oneshot::Receiver<Result<String, String>>>,
}

static OAUTH_FLOW_STATE: OnceLock<Mutex<Option<OAuthFlowState>>> = OnceLock::new();
const OAUTH_CALLBACK_PATH: &str = "/oauth-callback";
const MAX_HTTP_REQUEST_BYTES: usize = 32 * 1024;
const REQUEST_READ_TIMEOUT: Duration = Duration::from_secs(5);
const OAUTH_FLOW_WAIT_TIMEOUT: Duration = Duration::from_secs(10 * 60);

fn get_oauth_flow_state() -> &'static Mutex<Option<OAuthFlowState>> {
    OAUTH_FLOW_STATE.get_or_init(|| Mutex::new(None))
}

fn oauth_success_html() -> &'static str {
    "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\r\n\
    <html>\
    <body style='font-family: sans-serif; text-align: center; padding: 50px; background: #0d1117; color: #fff;'>\
        <h1 style='color: #4ade80;'>✅ Authorization Successful!</h1>\
        <p>You can close this window and return to the app.</p>\
        <script>setTimeout(function() { window.close(); }, 2000);</script>\
    </body>\
    </html>"
}

fn oauth_fail_html(message: &str) -> String {
    format!(
        "HTTP/1.1 400 Bad Request\r\nContent-Type: text/html; charset=utf-8\r\n\r\n\
    <html>\
    <body style='font-family: sans-serif; text-align: center; padding: 50px; background: #0d1117; color: #fff;'>\
        <h1 style='color: #f87171;'>❌ Authorization Failed</h1>\
        <p>{}</p>\
    </body>\
    </html>",
        message
    )
}

fn oauth_not_found_response() -> &'static str {
    "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain; charset=utf-8\r\n\r\nNot Found"
}

fn oauth_options_response() -> &'static str {
    "HTTP/1.1 200 OK\r\n\
    Access-Control-Allow-Origin: *\r\n\
    Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n\
    Access-Control-Allow-Headers: Content-Type\r\n\
    Content-Length: 0\r\n\r\n"
}

fn clear_oauth_flow_state() {
    if let Ok(mut lock) = get_oauth_flow_state().lock() {
        *lock = None;
    }
}

async fn read_http_request(stream: &mut tokio::net::TcpStream) -> Result<String, String> {
    let mut buffer = Vec::with_capacity(4096);
    let mut chunk = [0u8; 2048];

    loop {
        let bytes_read = timeout(REQUEST_READ_TIMEOUT, stream.read(&mut chunk))
            .await
            .map_err(|_| "Timed out while reading OAuth callback request".to_string())?
            .map_err(|e| format!("Failed to read OAuth callback request: {}", e))?;

        if bytes_read == 0 {
            break;
        }

        buffer.extend_from_slice(&chunk[..bytes_read]);
        if buffer.windows(4).any(|window| window == b"\r\n\r\n")
            || buffer.len() >= MAX_HTTP_REQUEST_BYTES
        {
            break;
        }
    }

    if buffer.is_empty() {
        return Err("OAuth callback request is empty".to_string());
    }

    Ok(String::from_utf8_lossy(&buffer).into_owned())
}

fn parse_request_target(request: &str) -> Result<(String, String), String> {
    let request_line = request
        .lines()
        .next()
        .ok_or_else(|| "OAuth callback request line is empty".to_string())?;
    let mut parts = request_line.split_whitespace();
    let method = parts
        .next()
        .ok_or_else(|| "OAuth callback request missing method".to_string())?;
    let target = parts
        .next()
        .ok_or_else(|| "OAuth callback request missing target".to_string())?;

    Ok((method.to_string(), target.to_string()))
}

async fn process_callback_request(
    stream: &mut tokio::net::TcpStream,
    port: u16,
    expected_state: &str,
) -> Option<Result<String, String>> {
    let request = match read_http_request(stream).await {
        Ok(request) => request,
        Err(err) => {
            let response = oauth_fail_html("Failed to read callback request. Please return to the app and retry.");
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.flush().await;
            return Some(Err(err));
        }
    };

    let (method, target) = match parse_request_target(&request) {
        Ok(parsed) => parsed,
        Err(err) => {
            let response = oauth_fail_html("Invalid callback request format. Please return to the app and retry.");
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.flush().await;
            return Some(Err(err));
        }
    };

    if method.eq_ignore_ascii_case("OPTIONS") {
        let _ = stream.write_all(oauth_options_response().as_bytes()).await;
        let _ = stream.flush().await;
        return None;
    }

    let callback_url = match if target.starts_with("http://") || target.starts_with("https://") {
        Url::parse(&target)
    } else {
        Url::parse(&format!("http://localhost:{}{}", port, target))
    } {
        Ok(url) => url,
        Err(_) => {
            let response = oauth_fail_html("Failed to parse callback URL. Please return to the app and retry.");
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.flush().await;
            return Some(Err("Failed to parse OAuth callback URL".to_string()));
        }
    };

    if callback_url.path() != OAUTH_CALLBACK_PATH {
        let _ = stream
            .write_all(oauth_not_found_response().as_bytes())
            .await;
        let _ = stream.flush().await;
        return None;
    }

    let mut code = None;
    let mut state = None;
    for (key, value) in callback_url.query_pairs() {
        match key.as_ref() {
            "code" if code.is_none() => code = Some(value.into_owned()),
            "state" if state.is_none() => state = Some(value.into_owned()),
            _ => {}
        }
    }

    let Some(code) = code.filter(|value| !value.trim().is_empty()) else {
        let response = oauth_fail_html("Authorization code was not found. Please return to the app and retry.");
        let _ = stream.write_all(response.as_bytes()).await;
        let _ = stream.flush().await;
        return Some(Err("Authorization code was not found in callback".to_string()));
    };

    let Some(state) = state.filter(|value| !value.trim().is_empty()) else {
        let response = oauth_fail_html("OAuth state was not found. Please return to the app and retry.");
        let _ = stream.write_all(response.as_bytes()).await;
        let _ = stream.flush().await;
        return Some(Err("OAuth state was not found in callback".to_string()));
    };

    if state != expected_state {
        let response = oauth_fail_html("OAuth state validation failed. Please return to the app and authorize again.");
        let _ = stream.write_all(response.as_bytes()).await;
        let _ = stream.flush().await;
        return Some(Err("OAuth state validation failed".to_string()));
    }

    let _ = stream.write_all(oauth_success_html().as_bytes()).await;
    let _ = stream.flush().await;

    Some(Ok(code))
}

async fn ensure_oauth_flow_prepared(app_handle: &tauri::AppHandle) -> Result<String, String> {
    use tauri::Emitter;

    if let Ok(state) = get_oauth_flow_state().lock() {
        if let Some(s) = state.as_ref() {
            return Ok(s.auth_url.clone());
        }
    }

    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("Failed to bind local port: {}", e))?;

    let port = listener
        .local_addr()
        .map_err(|e| format!("Failed to read local port: {}", e))?
        .port();

    let redirect_uri = format!("http://localhost:{}/oauth-callback", port);
    let state_token = uuid::Uuid::new_v4().to_string();
    let auth_url = oauth::get_auth_url(&redirect_uri, Some(&state_token));

    let (cancel_tx, cancel_rx) = watch::channel(false);
    let (code_tx, code_rx) = oneshot::channel::<Result<String, String>>();

    let code_tx = std::sync::Arc::new(tokio::sync::Mutex::new(Some(code_tx)));
    let app_handle_clone = app_handle.clone();

    let tx = code_tx.clone();
    let mut rx = cancel_rx;
    let expected_state = state_token.clone();
    tokio::spawn(async move {
        loop {
            let accept_result = tokio::select! {
                res = listener.accept() => Some(res),
                _ = rx.changed() => None,
            };

            let Some(accept_result) = accept_result else {
                break;
            };

            let Ok((mut stream, _)) = accept_result else {
                continue;
            };

            let result = process_callback_request(&mut stream, port, &expected_state).await;
            if let Some(result) = result {
                if let Some(sender) = tx.lock().await.take() {
                    let _ = app_handle_clone.emit("oauth-callback-received", ());
                    let _ = sender.send(result);
                }
                break;
            }
        }
    });

    if let Ok(mut state) = get_oauth_flow_state().lock() {
        *state = Some(OAuthFlowState {
            auth_url: auth_url.clone(),
            redirect_uri,
            cancel_tx,
            code_rx: Some(code_rx),
        });
    }

    let _ = app_handle.emit("oauth-url-generated", &auth_url);

    Ok(auth_url)
}

/// 预生成 OAuth URL
pub async fn prepare_oauth_url(app_handle: tauri::AppHandle) -> Result<String, String> {
    ensure_oauth_flow_prepared(&app_handle).await
}

/// 取消当前的 OAuth 流程
pub fn cancel_oauth_flow() {
    if let Ok(mut state) = get_oauth_flow_state().lock() {
        if let Some(s) = state.take() {
            let _ = s.cancel_tx.send(true);
        }
    }
}

/// 启动 OAuth 流程并等待回调
pub async fn start_oauth_flow(
    app_handle: tauri::AppHandle,
) -> Result<oauth::TokenResponse, String> {
    let auth_url = ensure_oauth_flow_prepared(&app_handle).await?;

    use tauri_plugin_opener::OpenerExt;
    app_handle
        .opener()
        .open_url(&auth_url, None::<String>)
        .map_err(|e| {
            cancel_oauth_flow();
            format!("Failed to open browser: {}", e)
        })?;

    let (code_rx, redirect_uri) = {
        let mut lock = get_oauth_flow_state()
            .lock()
            .map_err(|_| "OAuth state lock is poisoned".to_string())?;
        let Some(state) = lock.as_mut() else {
            return Err("OAuth state does not exist".to_string());
        };
        let rx = state
            .code_rx
            .take()
            .ok_or_else(|| "OAuth authorization is already in progress".to_string())?;
        (rx, state.redirect_uri.clone())
    };

    let callback_result = timeout(OAUTH_FLOW_WAIT_TIMEOUT, code_rx).await;
    let code = match callback_result {
        Ok(Ok(Ok(code))) => code,
        Ok(Ok(Err(e))) => {
            clear_oauth_flow_state();
            return Err(e);
        }
        Ok(Err(_)) => {
            clear_oauth_flow_state();
            return Err("Failed while waiting for OAuth callback".to_string());
        }
        Err(_) => {
            cancel_oauth_flow();
            return Err("Timed out waiting for OAuth callback, please retry".to_string());
        }
    };

    clear_oauth_flow_state();

    oauth::exchange_code(&code, &redirect_uri).await
}

/// 完成 OAuth 流程（不打开浏览器）

pub async fn complete_oauth_flow(
    app_handle: tauri::AppHandle,
) -> Result<oauth::TokenResponse, String> {
    let _ = ensure_oauth_flow_prepared(&app_handle).await?;

    let (code_rx, redirect_uri) = {
        let mut lock = get_oauth_flow_state()
            .lock()
            .map_err(|_| "OAuth state lock is poisoned".to_string())?;
        let Some(state) = lock.as_mut() else {
            return Err("OAuth state does not exist".to_string());
        };
        let rx = state
            .code_rx
            .take()
            .ok_or_else(|| "OAuth authorization is already in progress".to_string())?;
        (rx, state.redirect_uri.clone())
    };

    let callback_result = timeout(OAUTH_FLOW_WAIT_TIMEOUT, code_rx).await;
    let code = match callback_result {
        Ok(Ok(Ok(code))) => code,
        Ok(Ok(Err(e))) => {
            clear_oauth_flow_state();
            return Err(e);
        }
        Ok(Err(_)) => {
            clear_oauth_flow_state();
            return Err("Failed while waiting for OAuth callback".to_string());
        }
        Err(_) => {
            cancel_oauth_flow();
            return Err("Timed out waiting for OAuth callback, please retry".to_string());
        }
    };

    clear_oauth_flow_state();

    oauth::exchange_code(&code, &redirect_uri).await
}

