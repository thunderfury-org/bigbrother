<script lang="ts">
  import CheckCircle2 from '@lucide/svelte/icons/check-circle-2';
  import AlertCircle from '@lucide/svelte/icons/alert-circle';
  import Info from '@lucide/svelte/icons/info';
  import AlertTriangle from '@lucide/svelte/icons/triangle-alert';
  import X from '@lucide/svelte/icons/x';
  import { toasts } from './toast.svelte';
</script>

<div class="toast-container" aria-live="polite" aria-atomic="true">
  {#each toasts.items as item (item.id)}
    <div class="toast toast-{item.type}" role="alert">
      <div class="toast-icon">
        {#if item.type === 'success'}
          <CheckCircle2 size={16} class="text-emerald-400" />
        {:else if item.type === 'error'}
          <AlertCircle size={16} class="text-rose-400" />
        {:else if item.type === 'warning'}
          <AlertTriangle size={16} class="text-amber-400" />
        {:else}
          <Info size={16} class="text-sky-400" />
        {/if}
      </div>
      <div class="toast-message">{item.message}</div>
      <button
        type="button"
        class="toast-close"
        onclick={() => toasts.dismiss(item.id)}
        aria-label="关闭通知"
      >
        <X size={14} />
      </button>
    </div>
  {/each}
</div>

<style>
  .toast-container {
    position: fixed;
    bottom: 24px;
    right: 24px;
    z-index: 9999;
    display: flex;
    flex-direction: column;
    gap: 8px;
    max-width: calc(100vw - 32px);
    width: 380px;
    pointer-events: none;
  }

  .toast {
    pointer-events: auto;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 14px;
    background: rgba(19, 27, 38, 0.94);
    backdrop-filter: blur(12px);
    border: 1px solid var(--color-bb-line);
    border-radius: 8px;
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.6), 0 0 1px rgba(255, 255, 255, 0.2);
    color: var(--color-bb-ink);
    font-size: 13px;
    font-weight: 500;
    animation: toast-slide 0.22s cubic-bezier(0.16, 1, 0.3, 1);
  }

  .toast-success {
    border-color: rgba(34, 197, 94, 0.3);
  }

  .toast-error {
    border-color: rgba(239, 68, 68, 0.35);
  }

  .toast-warning {
    border-color: rgba(245, 158, 11, 0.35);
  }

  .toast-info {
    border-color: rgba(56, 189, 248, 0.3);
  }

  .toast-icon {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .toast-message {
    flex: 1;
    min-width: 0;
    line-height: 1.4;
    word-break: break-word;
  }

  .toast-close {
    flex-shrink: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 22px;
    height: 22px;
    border-radius: 4px;
    border: none;
    background: transparent;
    color: var(--color-bb-muted);
    cursor: pointer;
    transition: all 0.15s ease;
  }

  .toast-close:hover {
    color: var(--color-bb-ink);
    background: rgba(255, 255, 255, 0.08);
  }

  @keyframes toast-slide {
    from {
      opacity: 0;
      transform: translateY(12px) scale(0.96);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }

  @media (max-width: 640px) {
    .toast-container {
      bottom: 16px;
      right: 16px;
      left: 16px;
      width: auto;
    }
  }
</style>
