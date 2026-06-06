export type SiteId = "youtube" | "facebook" | "instagram" | "x" | "generic";

export interface SiteAdapter {
  readonly id: SiteId;
  keyFromUrl(url: string): string | null;
  mediaKey(): string | null;
  pickVideo(): HTMLMediaElement | null;
  isPlayerVideo(el: EventTarget | null): el is HTMLMediaElement;
  mountButton(): void;
  onNavigate(cb: () => void): () => void;
}

export interface AdapterClass {
  new (): SiteAdapter;
  matches(host: string): boolean;
}
