/**
 * Web uploader glue. Sends the selected file's audio to the Sukoon cloud endpoint and offers the
 * cleaned result for download. Scaffold — the upload/remux details land with Phase 2.
 */

const CLOUD_URL = import.meta.env.VITE_SUKOON_CLOUD_URL ?? "";

const input = document.querySelector<HTMLInputElement>("#file");

input?.addEventListener("change", async () => {
  const file = input.files?.[0];
  if (!file || !CLOUD_URL) return;
  // Real flow: extract audio (ffmpeg.wasm), POST to CLOUD_URL, remux locally, download.
  // The cloud never receives or stores the video; audio is deleted server-side after processing.
  console.info(`Would clean ${file.name} via ${CLOUD_URL}`);
});
