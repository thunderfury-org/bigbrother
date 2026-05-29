<script lang="ts">
  import { Router, Route, Link } from 'svelte-routing';
  import ImportsPage from './routes/ImportsPage.svelte';
  import FilesPage from './routes/FilesPage.svelte';

  export let url = '';
</script>

<div class="shell">
  <!-- Film grain overlay -->
  <div class="grain" aria-hidden="true">
    <svg>
      <filter id="grain-filter">
        <feTurbulence type="fractalNoise" baseFrequency="0.65" numOctaves="3" stitchTiles="stitch"/>
        <feColorMatrix type="saturate" values="0"/>
      </filter>
      <rect width="100%" height="100%" filter="url(#grain-filter)"/>
    </svg>
  </div>

  <Router {url}>
    <!-- Cinematic header -->
    <header class="header">
      <div class="header-inner">
        <!-- Film strip perforations -->
        <div class="film-strip" aria-hidden="true">
          {#each { length: 20 } as _}
            <span class="perf"></span>
          {/each}
        </div>

        <nav class="nav">
          <Link to="/imports" class="logo">
            <span class="logo-b">B</span>IG<span class="logo-b">B</span>ROTHER
          </Link>
          <div class="nav-links">
            <Link to="/imports" class="nav-link">
              <svg viewBox="0 0 20 20" fill="currentColor" width="16" height="16">
                <path d="M4 3a2 2 0 100 4h12a2 2 0 100-4H4zM3 9a1 1 0 000 2v5a2 2 0 002 2h10a2 2 0 002-2v-5a1 1 0 100-2H3zm5 2a1 1 0 011-1h2a1 1 0 110 2H9a1 1 0 01-1-1z"/>
              </svg>
              <span>导入历史</span>
            </Link>
            <Link to="/files" class="nav-link">
              <svg viewBox="0 0 20 20" fill="currentColor" width="16" height="16">
                <path d="M2 6a2 2 0 012-2h5l2 2h5a2 2 0 012 2v6a2 2 0 01-2 2H4a2 2 0 01-2-2V6z"/>
              </svg>
              <span>文件索引</span>
            </Link>
          </div>
        </nav>
      </div>
    </header>

    <!-- Page content -->
    <main class="main">
      <Route path="/imports/*"><ImportsPage /></Route>
      <Route path="/files"><FilesPage /></Route>
      <Route path="/">
        <div class="home-redirect">
          <p class="home-eyebrow">MEDIA LIBRARY CONSOLE</p>
          <h1 class="home-title">欢迎回来</h1>
          <Link to="/imports" class="home-cta">
            进入导入历史
            <svg viewBox="0 0 20 20" fill="currentColor" width="16" height="16">
              <path fill-rule="evenodd" d="M10.293 3.293a1 1 0 011.414 0l6 6a1 1 0 010 1.414l-6 6a1 1 0 01-1.414-1.414L14.586 11H3a1 1 0 110-2h11.586l-4.293-4.293a1 1 0 010-1.414z" clip-rule="evenodd"/>
            </svg>
          </Link>
        </div>
      </Route>
    </main>
  </Router>
</div>

<style>
  .shell {
    min-height: 100vh;
    background: linear-gradient(175deg, var(--color-bb-void) 0%, var(--color-bb-night) 30%, var(--color-bb-void) 100%);
    color: var(--color-bb-text);
    font-family: var(--font-body);
    -webkit-font-smoothing: antialiased;
  }

  /* ── Film grain overlay ── */
  .grain {
    position: fixed;
    inset: 0;
    pointer-events: none;
    z-index: 50;
    opacity: 0.035;
    mix-blend-mode: overlay;
  }
  .grain svg {
    width: 100%;
    height: 100%;
  }

  /* ── Header ── */
  .header {
    position: sticky;
    top: 0;
    z-index: 40;
    background: linear-gradient(180deg, var(--color-bb-night) 0%, rgba(16, 16, 24, 0.92) 100%);
    backdrop-filter: blur(12px);
    -webkit-backdrop-filter: blur(12px);
    border-bottom: 1px solid color-mix(in srgb, var(--color-bb-gold) 20%, transparent);
  }

  .header-inner {
    max-width: 1280px;
    margin: 0 auto;
    padding: 0 24px;
    position: relative;
  }

  /* Film strip perforations */
  .film-strip {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    display: flex;
    justify-content: space-evenly;
    pointer-events: none;
    overflow: hidden;
  }
  .perf {
    width: 8px;
    height: 4px;
    background: color-mix(in srgb, var(--color-bb-gold) 8%, transparent);
    border-radius: 0 0 2px 2px;
    flex-shrink: 0;
  }

  .nav {
    display: flex;
    align-items: center;
    gap: 32px;
    padding: 16px 0;
  }

  :global(.logo) {
    font-family: var(--font-display);
    font-size: 24px;
    letter-spacing: 0.15em;
    color: var(--color-bb-cream);
    text-decoration: none;
    transition: color 0.3s ease;
    flex-shrink: 0;
  }
  :global(.logo):hover {
    color: var(--color-bb-gold-light);
  }
  .logo-b {
    color: var(--color-bb-gold);
  }

  .nav-links {
    display: flex;
    gap: 4px;
  }

  :global(.nav-link) {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 8px 14px;
    border-radius: 6px;
    font-size: 14px;
    font-weight: 500;
    color: var(--color-bb-text-muted);
    text-decoration: none;
    transition: all 0.25s ease;
  }
  :global(.nav-link):hover {
    color: var(--color-bb-gold-light);
    background: color-mix(in srgb, var(--color-bb-gold) 8%, transparent);
  }

  /* Active link styling via svelte-routing's aria-current */
  :global([aria-current="page"].nav-link) {
    color: var(--color-bb-gold);
    background: color-mix(in srgb, var(--color-bb-gold) 12%, transparent);
  }

  :global(.nav-link) svg {
    flex-shrink: 0;
    opacity: 0.7;
  }
  :global(.nav-link):hover svg {
    opacity: 1;
  }

  /* ── Main content ── */
  .main {
    max-width: 1280px;
    margin: 0 auto;
    padding: 32px 24px 64px;
  }

  /* ── Home redirect ── */
  .home-redirect {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    min-height: 50vh;
    text-align: center;
  }
  .home-eyebrow {
    font-family: var(--font-display);
    font-size: 14px;
    letter-spacing: 0.3em;
    color: var(--color-bb-gold-dim);
    margin-bottom: 12px;
  }
  .home-title {
    font-family: var(--font-display);
    font-size: 48px;
    letter-spacing: 0.1em;
    color: var(--color-bb-cream);
    margin-bottom: 32px;
  }
  :global(.home-cta) {
    display: inline-flex;
    align-items: center;
    gap: 8px;
    padding: 14px 32px;
    background: linear-gradient(135deg, var(--color-bb-gold-dim), var(--color-bb-gold));
    color: var(--color-bb-void);
    font-family: var(--font-display);
    font-size: 18px;
    letter-spacing: 0.12em;
    text-decoration: none;
    border-radius: 4px;
    transition: all 0.3s ease;
  }
  :global(.home-cta):hover {
    background: linear-gradient(135deg, var(--color-bb-gold), var(--color-bb-gold-light));
    transform: translateY(-2px);
    box-shadow: 0 8px 32px color-mix(in srgb, var(--color-bb-gold) 25%, transparent);
  }
</style>
