<script lang="ts">
  import { Router, Route, Link } from 'svelte-routing';
  import Inbox from '@lucide/svelte/icons/inbox';
  import FolderSearch from '@lucide/svelte/icons/folder-search';
  import FolderTree from '@lucide/svelte/icons/folder-tree';
  import ListPlus from '@lucide/svelte/icons/list-plus';
  import ImportsPage from './routes/ImportsPage.svelte';
  import FilesPage from './routes/FilesPage.svelte';
  import MediaDirsPage from './routes/MediaDirsPage.svelte';
  import SubscriptionsPage from './routes/SubscriptionsPage.svelte';

  export let url = '';
</script>

<div class="shell">
  <Router {url}>
    <header class="header">
      <div class="header-inner">
        <nav class="nav">
          <Link to="/imports" class="logo">BigBrother</Link>
          <div class="nav-links">
            <Link
              to="/imports"
              getProps={({ location }) => ({
                class: location.pathname === '/' || location.pathname.startsWith('/imports')
                  ? 'nav-link is-active'
                  : 'nav-link',
              })}
            >
              <Inbox size={15} />
              <span>导入历史</span>
            </Link>
            <Link to="/files" class="nav-link">
              <FolderSearch size={15} />
              <span>文件索引</span>
            </Link>
            <Link to="/media" class="nav-link">
              <FolderTree size={15} />
              <span>媒体目录</span>
            </Link>
            <Link to="/subscriptions" class="nav-link">
              <ListPlus size={15} />
              <span>订阅管理</span>
            </Link>
          </div>
        </nav>
      </div>
    </header>

    <main class="main">
      <Route path="/imports/*"><ImportsPage /></Route>
      <Route path="/files"><FilesPage /></Route>
      <Route path="/media"><MediaDirsPage /></Route>
      <Route path="/subscriptions"><SubscriptionsPage /></Route>
      <Route path="/"><ImportsPage /></Route>
    </main>
  </Router>
</div>

<style>
  .shell {
    min-height: 100vh;
    background: var(--color-bb-paper);
    color: var(--color-bb-ink);
    font-family: var(--font-sans);
  }

  .header {
    position: sticky;
    top: 0;
    z-index: 40;
    background: var(--color-bb-surface);
    border-bottom: 1px solid var(--color-bb-line);
  }

  .header-inner {
    max-width: 1280px;
    margin: 0 auto;
    padding: 0 24px;
  }

  .nav {
    display: flex;
    align-items: center;
    gap: 28px;
    min-height: 52px;
  }

  :global(.logo) {
    flex-shrink: 0;
    font-size: 18px;
    font-weight: 800;
    color: var(--color-bb-ink);
    text-decoration: none;
  }

  :global(.logo):hover {
    color: var(--color-bb-mark);
  }

  .nav-links {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 8px;
  }

  :global(.nav-link) {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 8px 2px;
    border-bottom: 2px solid transparent;
    font-size: 14px;
    font-weight: 500;
    color: var(--color-bb-muted);
    text-decoration: none;
  }

  :global(.nav-link):hover {
    color: var(--color-bb-ink);
  }

  :global(.nav-link[aria-current="page"]),
  :global(.nav-link.is-active) {
    color: var(--color-bb-ink);
    border-bottom-color: var(--color-bb-mark);
  }

  .main {
    max-width: 1280px;
    margin: 0 auto;
    padding: 24px 24px 64px;
  }

  @media (max-width: 768px) {
    .nav {
      flex-direction: column;
      align-items: flex-start;
      gap: 8px;
      padding: 12px 0;
    }
  }
</style>
