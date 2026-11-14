/**
 * Moves an element to <body>, so fixed positioning isn't trapped by a transformed ancestor (a pressed button,
 * a sliding panel). Svelte still removes the element when its block is destroyed.
 */
export function portal(node: HTMLElement, target: HTMLElement | string = 'body') {
  const host = typeof target === 'string' ? document.querySelector(target) : target;
  host?.appendChild(node);
  return {
    destroy() {
      node.remove();
    },
  };
}
