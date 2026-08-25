<script lang="ts">
  interface Props {
    width?: string;
    height?: string;
    rounded?: string;
    class?: string;
  }

  let {
    width = '100%',
    height = '1rem',
    rounded = '4px',
    class: className = '',
  }: Props = $props();
</script>

<div
  class="skeleton-placeholder {className}"
  style:width
  style:height
  style:border-radius={rounded}
  aria-hidden="true"
></div>

<style>
  .skeleton-placeholder {
    display: inline-block;
    background: linear-gradient(
      90deg,
      rgba(255, 255, 255, 0.04) 25%,
      rgba(255, 255, 255, 0.09) 50%,
      rgba(255, 255, 255, 0.04) 75%
    );
    background-size: 200% 100%;
    animation: skeleton-shimmer 1.6s infinite ease-in-out;
  }

  @keyframes skeleton-shimmer {
    0% {
      background-position: 200% 0;
    }
    100% {
      background-position: -200% 0;
    }
  }
</style>
