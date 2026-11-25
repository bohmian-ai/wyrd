/**
 * Interface icon paths on one 24px, 1.6px-stroke grammar, drawn by `Icon.svelte`.
 * The rail destinations keep their existing shapes; the rest name the
 * administration sections and controls.
 */
export const ICONS = {
  home: 'M3 11l9-7 9 7v9h-6v-6H9v6H3z',
  cards: 'M4 4h16v16H4zM4 9h16',
  repositories: 'M6 4a2 2 0 1 0 0 4 2 2 0 1 0 0-4M6 16a2 2 0 1 0 0 4 2 2 0 1 0 0-4M6 8v8M10 6h8v12h-8',
  observe: 'M3 12h4l3-7 4 14 3-7h4',
  verify: 'M5 12l5 5L20 7',
  query: 'M4 6c0-1.7 3.6-3 8-3s8 1.3 8 3-3.6 3-8 3-8-1.3-8-3zM4 6v12c0 1.7 3.6 3 8 3s8-1.3 8-3V6',
  settings: 'M12 9a3 3 0 1 0 0 6 3 3 0 1 0 0-6M12 2v3M12 19v3M2 12h3M19 12h3M4.9 4.9l2.1 2.1M17 17l2.1 2.1M4.9 19.1L7 17M17 7l2.1-2.1',
  people: 'M9 11a4 4 0 1 0 0-8 4 4 0 0 0 0 8zM2 21c0-4 3-6 7-6s7 2 7 6M16 3.5a4 4 0 0 1 0 7.5M18 15c2.5.5 4 2.5 4 6',
  service: 'M4 7h16v10H4zM8 11h.01M12 11h4M7 21h10M12 17v4',
  roles: 'M12 3l8 3v6c0 5-3.5 8-8 9-4.5-1-8-4-8-9V6z',
  signin: 'M15 3h4a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2h-4M10 17l5-5-5-5M15 12H3',
  secrets: 'M7 11V7a5 5 0 0 1 10 0v4M5 11h14v10H5zM12 15v2',
  integrations: 'M9 3v6M15 3v6M6 9h12v3a6 6 0 0 1-12 0zM12 18v3',
  overview: 'M4 4h7v7H4zM13 4h7v4h-7zM13 10h7v10h-7zM4 13h7v7H4z',
  workspaces: 'M3 7h18v13H3zM8 7V4h8v3',
  providers: 'M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20zM2 12h20M12 2c3 3 3 17 0 20M12 2c-3 3-3 17 0 20',
  mail: 'M3 5h18v14H3zM3 6l9 7 9-7',
  admins: 'M12 3l8 3v6c0 5-3.5 8-8 9-4.5-1-8-4-8-9V6zM9 12l2 2 4-4',
  moon: 'M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z',
  sun: 'M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8zM12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4',
  plus: 'M12 5v14M5 12h14',
  copy: 'M9 9h11v11H9zM5 15H4V4h11v1'
} as const;
