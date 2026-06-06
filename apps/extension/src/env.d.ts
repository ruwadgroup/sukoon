/// <reference types="vite/client" />

// Side-effect CSS imports (the popup's Tailwind entry) carry no types.
declare module "*.css";

// Self-hosted webfont CSS bundle (Outfit), imported for its side effect only.
declare module "@fontsource-variable/outfit";
