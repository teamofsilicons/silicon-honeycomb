import { createEffect, createSignal, For, onCleanup, onMount, type JSX } from "solid-js";
import "./arc.css";

// Solid adaptations of the free Arc components. See ARC_SOURCE.md and ARC_LICENSE.
export type StatusTone = "neutral" | "success" | "warning" | "danger" | "info";

export function StatusBadge(props: { tone?: StatusTone; children: JSX.Element }) {
  return <span class={`arc-badge ${props.tone || "neutral"}`}>{props.children}</span>;
}

export function SegmentedControl(props: {
  value: string;
  onChange: (value: string) => void;
  options: { value: string; label: JSX.Element }[];
  label: string;
}) {
  let track!: HTMLDivElement;
  const buttons = new Map<string, HTMLButtonElement>();
  const [selection, setSelection] = createSignal({ left: 3, width: 0 });
  const selectedIndex = () => Math.max(0, props.options.findIndex(option => option.value === props.value));
  function measure() {
    const button = buttons.get(props.value);
    if (button) setSelection({ left: button.offsetLeft, width: button.offsetWidth });
  }
  createEffect(() => {
    props.value;
    props.options;
    queueMicrotask(() => {
      measure();
      const button = buttons.get(props.value);
      if (!button || !track) return;
      const start = button.offsetLeft - 3;
      const end = start + button.offsetWidth + 6;
      if (start < track.scrollLeft) track.scrollLeft = start;
      else if (end > track.scrollLeft + track.clientWidth) track.scrollLeft = end - track.clientWidth;
    });
  });
  onMount(() => {
    const observer = new ResizeObserver(measure);
    observer.observe(track);
    measure();
    onCleanup(() => observer.disconnect());
  });
  function keydown(event: KeyboardEvent) {
    const last = props.options.length - 1, current = selectedIndex();
    const index = ["ArrowRight", "ArrowDown"].includes(event.key) ? (current + 1) % props.options.length
      : ["ArrowLeft", "ArrowUp"].includes(event.key) ? (current === 0 ? last : current - 1)
      : event.key === "Home" ? 0 : event.key === "End" ? last : -1;
    const option = props.options[index];
    if (!option) return;
    event.preventDefault();
    props.onChange(option.value);
    buttons.get(option.value)?.focus({ preventScroll: true });
  }
  return <div class="arc-segmented" role="group" aria-label={props.label}>
    <div class="arc-segmented-track" ref={track}>
      <span class="arc-segmented-selection" aria-hidden="true" style={{ width: `${selection().width}px`, transform: `translateX(${selection().left}px)` }} />
      <For each={props.options}>{(option, index) => <button
        ref={element => buttons.set(option.value, element)} type="button"
        aria-pressed={props.value === option.value} tabIndex={index() === selectedIndex() ? 0 : -1}
        onClick={() => props.onChange(option.value)} onKeyDown={keydown}
      >{option.label}</button>}</For>
    </div>
  </div>;
}
