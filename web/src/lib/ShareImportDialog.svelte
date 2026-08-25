<script lang="ts">
  import X from '@lucide/svelte/icons/x';
  import Link from '@lucide/svelte/icons/link';
  import Download from '@lucide/svelte/icons/download';
  import { ApiError, importShareUrl, type ShareImportResult } from './api';
  import ImportSummaryItems from './ImportSummaryItems.svelte';
  import { toasts } from './toast.svelte';
  import { statusLabel } from './importDisplay';

  let open = $state(false);
  let importing = $state(false);
  let url = $state('');
  let description = $state('');
  let formError = $state('');
  let result: ShareImportResult | null = $state(null);

  function openDialog() {
    open = true;
    importing = false;
    formError = '';
    result = null;
  }

  function closeDialog() {
    if (importing) return;
    open = false;
    url = '';
    description = '';
    formError = '';
    result = null;
  }

  async function submit() {
    const shareUrl = url.trim();
    if (!shareUrl || importing) return;
    importing = true;
    formError = '';
    result = null;
    try {
      const imported = await importShareUrl(shareUrl, description.trim() || undefined);
      result = imported;
      if (imported.status === 'succeeded') {
        toasts.success(`已导入 ${imported.title || shareUrl}`);
      } else if (imported.status === 'skipped') {
        toasts.info(imported.error || '分享未产生可导入媒体');
      } else {
        toasts.error(imported.error || '导入失败');
      }
    } catch (err) {
      formError = err instanceof ApiError ? `导入失败: ${err.body}` : String(err);
      toasts.error(formError);
    } finally {
      importing = false;
    }
  }

  $effect(() => {
    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && open) {
        closeDialog();
      }
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  });
</script>

<button type="button" class="btn btn-primary btn-sm header-import" onclick={openDialog}>
  <Link size={14} />
  <span>导入链接</span>
</button>

{#if open}
  <!-- svelte-ignore a11y_no_static_element_interactions a11y_click_events_have_key_events -->
  <div
    class="modal-backdrop"
    role="presentation"
    onclick={(event) => {
      if (event.target === event.currentTarget) closeDialog();
    }}
  >
    <div
      class="modal"
      role="dialog"
      tabindex="-1"
      aria-modal="true"
      aria-labelledby="share-import-title"
    >
      <header class="modal-header">
        <h2 id="share-import-title" class="modal-title">
          {#if importing}
            正在导入中
          {:else if result}
            导入结果
          {:else}
            导入分享链接
          {/if}
        </h2>
        <button
          type="button"
          class="drawer-close"
          aria-label="关闭"
          disabled={importing}
          onclick={closeDialog}
        >
          <X size={16} />
        </button>
      </header>
      <div class="modal-body">
        {#if importing}
          <div class="loading">
            <div class="loading-bar"></div>
            <p>正在解析并导入分享链接，大分享可能需要几分钟…</p>
          </div>
        {:else if result}
          <div class="import-summary" data-status={result.status}>
            <div class="result-row">
              <span class="status status-{result.status}">
                <span class="pulse-dot"></span>
                {statusLabel(result.status)}
              </span>
              {#if !result.summary}
                <span class="cell-title">{result.title || result.url}</span>
              {/if}
            </div>
            {#if result.summary}
              <ImportSummaryItems summary={result.summary} />
            {/if}
            <div class="mono cell-sub" style="margin-top: 6px;">{result.url}</div>
            {#if result.error}
              <div class="banner-error" style="margin-top: 8px;">{result.error}</div>
            {/if}
          </div>
          <div class="dialog-actions">
            <button type="button" class="btn btn-primary" onclick={closeDialog}>完成</button>
          </div>
        {:else}
          <form class="share-form" onsubmit={(event) => { event.preventDefault(); void submit(); }}>
            <label class="field">
              <span class="field-label">分享链接</span>
              <textarea
                class="input share-url-input"
                bind:value={url}
                placeholder="https://www.123684.com/s/xxxx?pwd= 或 189 / 115 分享链接"
                rows="3"
                disabled={importing}
              ></textarea>
            </label>
            <label class="field">
              <span class="field-label">备注（可选）</span>
              <input
                class="input"
                bind:value={description}
                placeholder="写入文件索引的描述，便于之后搜索"
                disabled={importing}
              />
            </label>
            {#if formError}
              <div class="banner-error">{formError}</div>
            {/if}
            <div class="dialog-actions">
              <button type="button" class="btn btn-ghost" onclick={closeDialog}>取消</button>
              <button type="submit" class="btn btn-primary" disabled={!url.trim()}>
                <Download size={15} />
                <span>导入</span>
              </button>
            </div>
          </form>
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .header-import {
    white-space: nowrap;
  }

  .share-form {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }

  .share-url-input {
    width: 100%;
    min-height: 88px;
    resize: vertical;
    line-height: 1.45;
  }

  .dialog-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    margin-top: 8px;
  }
</style>
