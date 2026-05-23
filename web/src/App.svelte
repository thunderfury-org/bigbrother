<script lang="ts">
  import { Router, Route, Link } from 'svelte-routing';
  import ImportsPage from './routes/ImportsPage.svelte';
  import FilesPage from './routes/FilesPage.svelte';

  export let url = '';
</script>

<Router {url}>
  <header class="border-b border-slate-200 bg-white">
    <nav class="mx-auto flex max-w-6xl items-center gap-6 px-6 py-3 text-sm">
      <span class="font-semibold text-slate-900">BigBrother</span>
      <Link to="/imports" class="text-slate-600 hover:text-slate-900">导入历史</Link>
      <Link to="/files" class="text-slate-600 hover:text-slate-900">文件索引</Link>
    </nav>
  </header>
  <main class="mx-auto max-w-6xl px-6 py-6">
    <Route path="/imports/*"><ImportsPage /></Route>
    <Route path="/files"><FilesPage /></Route>
    <Route path="/">
      <div class="text-sm text-slate-600">
        <Link to="/imports" class="text-blue-600 hover:underline">前往导入历史 →</Link>
      </div>
    </Route>
  </main>
</Router>
