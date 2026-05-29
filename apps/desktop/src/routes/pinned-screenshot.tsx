import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { createMemo, createSignal, onCleanup, onMount, Show } from "solid-js";
import IconLucidePin from "~icons/lucide/pin";
import IconLucideX from "~icons/lucide/x";

export default function PinnedScreenshotRoute() {
	const [path, setPath] = createSignal<string | null>(null);
	const [error, setError] = createSignal<string | null>(null);
	const [imageReady, setImageReady] = createSignal(false);
	const currentWindow = getCurrentWindow();
	const imageSrc = createMemo(() => {
		const value = path();
		return value ? convertFileSrc(value) : null;
	});
	const visibleImageSrc = createMemo(() => (error() ? null : imageSrc()));

	onMount(() => {
		invoke<string>("get_pinned_screenshot_window_path")
			.then(setPath)
			.catch((value: unknown) =>
				setError(value instanceof Error ? value.message : String(value)),
			);

		const handleKeyDown = (event: KeyboardEvent) => {
			if (event.key === "Escape") void currentWindow.close();
		};

		window.addEventListener("keydown", handleKeyDown);
		onCleanup(() => window.removeEventListener("keydown", handleKeyDown));
	});

	return (
		<div class="flex h-screen w-screen flex-col overflow-hidden bg-gray-12 text-gray-1">
			<header
				data-tauri-drag-region
				class="flex h-[42px] shrink-0 items-center justify-between border-white/10 border-b bg-black/90 px-2"
			>
				<div class="flex min-w-0 items-center gap-2 px-1">
					<div class="flex size-6 shrink-0 items-center justify-center rounded bg-white/10 text-blue-8">
						<IconLucidePin class="size-3.5" />
					</div>
					<div class="flex min-w-0 items-baseline gap-2">
						<span class="shrink-0 text-[12px] font-medium text-white">
							Pinned
						</span>
						<Show when={path()}>
							{(value) => (
								<span class="truncate text-[11px] text-white/55">
									{value().split(/[/\\]/).pop()}
								</span>
							)}
						</Show>
					</div>
				</div>
				<button
					type="button"
					class="flex size-8 items-center justify-center rounded text-white/65 transition-colors hover:bg-white/10 hover:text-white"
					aria-label="Close pinned screenshot"
					onClick={() => void currentWindow.close()}
				>
					<IconLucideX class="size-4" />
				</button>
			</header>

			<main class="relative flex min-h-0 flex-1 items-center justify-center bg-black">
				<Show when={error()}>
					{(message) => (
						<div class="max-w-[80%] text-center text-[12px] text-white/70">
							{message()}
						</div>
					)}
				</Show>
				<Show when={visibleImageSrc()}>
					{(src) => (
						<img
							src={src()}
							alt="Pinned screenshot"
							class="h-full w-full object-contain"
							classList={{ "opacity-0": !imageReady() }}
							draggable={false}
							onLoad={() => setImageReady(true)}
							onError={() => setError("Failed to load screenshot")}
						/>
					)}
				</Show>
			</main>
		</div>
	);
}
