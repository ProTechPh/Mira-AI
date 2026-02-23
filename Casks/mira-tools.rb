cask "Mira-tools" do
  version "0.8.17"
  sha256 "c5d45af0dbe61c958446090b8160bb4c0eaf00ee2a1fe95601b2036079548d4e"

  url "https://github.com/ProTechPh/Mira-AI/releases/download/v#{version}/Mira.Tools_#{version}_universal.dmg",
      verified: "github.com/ProTechPh/Mira-AI/"
  name ""
  desc "Account manager for AI IDEs (Antigravity and Codex)"
  homepage "https://github.com/ProTechPh/Mira-AI"

  auto_updates true

  postflight do
    system_command "/usr/bin/xattr",
                   args: ["-cr", "#{appdir}/.app"],
                   sudo: true
  end

  app ".app"

  zap trash: [
    "~/Library/Application Support/com.protechph.mira-ai",
    "~/Library/Caches/com.protechph.mira-ai",
    "~/Library/Preferences/com.protechph.mira-ai.plist",
    "~/Library/Saved Application State/com.protechph.mira-ai.savedState",
  ]

  caveats <<~EOS
    The app is automatically quarantined by macOS. A postflight hook has been added to remove this quarantine.
    If you still encounter the "App is damaged" error, please run:
      sudo xattr -rd com.apple.quarantine "/Applications/.app"
  EOS
end
