import { defineConfig } from 'vitepress'

export default defineConfig({
  title: 'Rungu',
  description: 'Lightweight, self-hosted feedback board. Collect feature requests, bug reports, and suggestions with voting and prioritization.',
  lang: 'en',
  cleanUrls: true,
  head: [
    ['link', { rel: 'icon', href: '/favicon.svg' }],
    ['meta', { property: 'og:type', content: 'website' }],
    ['meta', { property: 'og:title', content: 'Rungu — Listen. Prioritize. Build.' }],
    ['meta', { property: 'og:description', content: 'Self-hosted feedback board with voting, comments, and multi-provider OAuth.' }],
    ['meta', { name: 'twitter:card', content: 'summary_large_image' }],
  ],
  themeConfig: {
    logo: '/favicon.svg',
    nav: [
      { text: 'Codecora', link: 'https://codecora.dev' },
      { text: 'Docs', link: '/getting-started' },
      { text: 'CLI Reference', link: '/cli-reference' },
      { text: 'Configuration', link: '/configuration' },
      { text: 'GitHub', link: 'https://github.com/codecoradev/rungu' },
    ],
    sidebar: [
      {
        text: 'Getting Started',
        items: [
          { text: 'Introduction', link: '/' },
          { text: 'Installation', link: '/getting-started' },
          { text: 'CLI Reference', link: '/cli-reference' },
          { text: 'Configuration', link: '/configuration' },
          { text: 'Docker', link: '/docker' },
        ],
      },
      {
        text: 'Features',
        items: [
          { text: 'Voting', link: '/features/voting' },
          { text: 'Comments & Threads', link: '/features/comments' },
          { text: 'Categories & Status', link: '/features/categories' },
          { text: 'Multi-Board', link: '/features/multi-board' },
          { text: 'Search & Sort', link: '/features/search' },
        ],
      },
      {
        text: 'Auth',
        items: [
          { text: 'Overview', link: '/auth/overview' },
          { text: 'Google OAuth', link: '/auth/google' },
          { text: 'GitHub OAuth', link: '/auth/github' },
          { text: 'Keycloak', link: '/auth/keycloak' },
        ],
      },
      {
        text: 'Integrations',
        items: [
          { text: 'MCP Server', link: '/integrations/mcp' },
          { text: 'REST API', link: '/integrations/api' },
        ],
      },
      {
        text: 'Development',
        items: [
          { text: 'Architecture', link: '/development/architecture' },
          { text: 'Contributing', link: '/development/contributing' },
          { text: 'Roadmap', link: '/development/roadmap' },
        ],
      },
    ],
    socialLinks: [{ icon: 'github', link: 'https://github.com/codecoradev/rungu' }],
    search: { provider: 'local' },
  },
})
