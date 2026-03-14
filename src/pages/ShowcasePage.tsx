import { useTranslation } from 'react-i18next';
import { ExternalLink, Github, Globe, Star } from 'lucide-react';
import './ShowcasePage.css';

interface ShowcaseItem {
  id: string;
  name: string;
  description: string;
  descriptionKey?: string;
  url: string;
  github?: string;
  tags: string[];
  featured?: boolean;
}

export function ShowcasePage() {
  const { t } = useTranslation();

  const showcaseItems: ShowcaseItem[] = [
    {
      id: 'kiro-account-manager',
      name: 'Kiro Account Manager',
      description: 'A powerful multi-account management tool for Kiro IDE with quick account switching, auto token refresh, group/tag management, machine ID management, API proxy service, and built-in chat interface',
      url: 'https://github.com/ProTechPh/Kiro-account-manager',
      github: 'https://github.com/ProTechPh/Kiro-account-manager',
      tags: ['Account Manager', 'Kiro IDE', 'Multi-Account', 'Proxy', 'Chat'],
      featured: true,
    },
    // Add more showcase items here
  ];

  const featuredItems = showcaseItems.filter((item) => item.featured);
  const regularItems = showcaseItems.filter((item) => !item.featured);

  return (
    <div className="showcase-page">
      <div className="showcase-header">
        <h1>{t('showcase.title', 'Third-Party Showcase')}</h1>
        <p className="showcase-subtitle">
          {t('showcase.subtitle', 'Discover integrations, tools, and services that work with Mira AI')}
        </p>
      </div>

      {featuredItems.length > 0 && (
        <section className="showcase-section">
          <h2 className="section-title">
            <Star size={20} />
            {t('showcase.featured', 'Featured')}
          </h2>
          <div className="showcase-grid featured">
            {featuredItems.map((item) => (
              <ShowcaseCard key={item.id} item={item} />
            ))}
          </div>
        </section>
      )}

      <section className="showcase-section">
        <h2 className="section-title">
          <Globe size={20} />
          {t('showcase.all', 'All Projects')}
        </h2>
        <div className="showcase-grid">
          {regularItems.map((item) => (
            <ShowcaseCard key={item.id} item={item} />
          ))}
        </div>
      </section>
    </div>
  );
}

function ShowcaseCard({ item }: { item: ShowcaseItem }) {
  const { t } = useTranslation();

  return (
    <div className={`showcase-card ${item.featured ? 'featured' : ''}`}>
      <div className="card-header">
        <h3 className="card-title">{item.name}</h3>
        {item.featured && (
          <span className="featured-badge">
            <Star size={14} />
            {t('showcase.featuredBadge', 'Featured')}
          </span>
        )}
      </div>

      <p className="card-description">
        {item.descriptionKey ? t(item.descriptionKey, item.description) : item.description}
      </p>

      <div className="card-tags">
        {item.tags.map((tag) => (
          <span key={tag} className="tag">
            {tag}
          </span>
        ))}
      </div>

      <div className="card-actions">
        <a
          href={item.url}
          target="_blank"
          rel="noopener noreferrer"
          className="card-link primary"
          title={t('showcase.visitWebsite', 'Visit Website')}
        >
          <Globe size={16} />
          {t('showcase.website', 'Website')}
          <ExternalLink size={14} />
        </a>
        {item.github && (
          <a
            href={item.github}
            target="_blank"
            rel="noopener noreferrer"
            className="card-link secondary"
            title={t('showcase.viewGithub', 'View on GitHub')}
          >
            <Github size={16} />
            {t('showcase.github', 'GitHub')}
            <ExternalLink size={14} />
          </a>
        )}
      </div>
    </div>
  );
}
