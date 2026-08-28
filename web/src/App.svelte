<script lang="ts">
  import { Router, Route, Link } from 'svelte-routing';
  import Inbox from '@lucide/svelte/icons/inbox';
  import Search from '@lucide/svelte/icons/search';
  import FolderTree from '@lucide/svelte/icons/folder-tree';
  import ListPlus from '@lucide/svelte/icons/list-plus';
  import ImportsPage from './routes/ImportsPage.svelte';
  import FilesPage from './routes/FilesPage.svelte';
  import MediaDirsPage from './routes/MediaDirsPage.svelte';
  import SubscriptionsPage from './routes/SubscriptionsPage.svelte';
  import ToastContainer from './lib/ToastContainer.svelte';
  import ShareImportDialog from './lib/ShareImportDialog.svelte';

  export let url = '';
</script>

<div class="shell">
  <ToastContainer />
  <Router {url}>
    <header class="header">
      <div class="header-inner">
        <nav class="nav">
          <Link to="/subscriptions" class="brand">
            <div class="logo-badge">B</div>
            <span class="logo-text">BigBrother</span>
          </Link>
          <div class="nav-links">
            <Link
              to="/subscriptions"
              getProps={({ location }) => ({
                class: location.pathname === '/' || location.pathname.startsWith('/subscriptions')
                  ? 'nav-link is-active'
                  : 'nav-link',
              })}
            >
              <ListPlus size={16} />
              <span>订阅管理</span>
            </Link>
            <Link
              to="/imports"
              getProps={({ location }) => ({
                class: location.pathname.startsWith('/imports')
                  ? 'nav-link is-active'
                  : 'nav-link',
              })}
            >
              <Inbox size={16} />
              <span>导入历史</span>
            </Link>
            <Link
              to="/files"
              getProps={({ location }) => ({
                class: location.pathname.startsWith('/files')
                  ? 'nav-link is-active'
                  : 'nav-link',
              })}
            >
              <Search size={16} />
              <span>搜索中心</span>
            </Link>
            <Link
              to="/media"
              getProps={({ location }) => ({
                class: location.pathname.startsWith('/media')
                  ? 'nav-link is-active'
                  : 'nav-link',
              })}
            >
              <FolderTree size={16} />
              <span>媒体目录</span>
            </Link>
          </div>
          <div class="header-right">
            <ShareImportDialog />
          </div>
        </nav>
      </div>
    </header>

    <main class="main">
      <Route path="/subscriptions"><SubscriptionsPage /></Route>
      <Route path="/imports/*"><ImportsPage /></Route>
      <Route path="/files"><FilesPage /></Route>
      <Route path="/media"><MediaDirsPage /></Route>
      <Route path="/"><SubscriptionsPage /></Route>
    </main>
  </Router>
</div>

<style>
  .shell {
    min-height: 100vh;
    background: var(--color-bb-paper);
    color: var(--color-bb-ink);
    font-family: var(--font-sans);
    display: flex;
    flex-direction: column;
  }

  .header {
    position: sticky;
    top: 0;
    z-index: 40;
    background: rgba(17, 23, 34, 0.85);
    backdrop-filter: blur(16px);
    border-bottom: 1px solid var(--color-bb-line);
    box-shadow: 0 4px 20px rgba(0, 0, 0, 0.4);
  }

  .header-inner {
    max-width: 2400px;
    margin: 0 auto;
    padding: 0 24px;
  }

  .nav {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 20px;
    min-height: 60px;
  }

  :global(.brand) {
    display: inline-flex;
    align-items: center;
    gap: 10px;
    text-decoration: none;
    flex-shrink: 0;
  }

  .logo-badge {
    width: 28px;
    height: 28px;
    border-radius: 8px;
    background: linear-gradient(135deg, #10b981 0%, #047857 100%);
    display: flex;
    align-items: center;
    justify-content: center;
    color: #ffffff;
    font-size: 14px;
    font-weight: 900;
    box-shadow: 0 0 16px rgba(16, 185, 129, 0.4);
  }

  .logo-text {
    font-size: 16px;
    font-weight: 750;
    color: #ffffff;
    letter-spacing: -0.3px;
  }

  .nav-links {
    display: flex;
    align-items: center;
    gap: 4px;
    background: rgba(0, 0, 0, 0.28);
    padding: 4px;
    border-radius: 8px;
    border: 1px solid var(--color-bb-line);
  }

  :global(.nav-link) {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 14px;
    border-radius: 6px;
    font-size: 13px;
    font-weight: 600;
    color: var(--color-bb-muted);
    text-decoration: none;
    transition: all 0.18s cubic-bezier(0.4, 0, 0.2, 1);
  }

  :global(.nav-link):hover {
    color: #ffffff;
    background: rgba(255, 255, 255, 0.05);
  }

  :global(.nav-link.is-active) {
    color: #ffffff;
    background: var(--color-bb-panel);
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.3);
    border: 1px solid rgba(255, 255, 255, 0.08);
  }

  .header-right {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .main {
    max-width: 2400px;
    width: 100%;
    margin: 0 auto;
    padding: 24px 24px 64px;
    flex: 1;
  }

  @media (max-width: 768px) {
    .nav {
      flex-direction: column;
      align-items: stretch;
      gap: 12px;
      padding: 12px 0;
      min-height: auto;
    }

    .header-right {
      justify-content: flex-end;
    }

    .nav-links {
      overflow-x: auto;
      justify-content: flex-start;
    }
  }
</style>
