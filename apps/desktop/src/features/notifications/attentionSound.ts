/** A short local cue. A locked audio context fails silently until the user permits playback. */
export function playAttentionTone(): void {
  if (typeof window.AudioContext !== 'function') return;
  try {
    const context = new AudioContext();
    const fallback = window.setTimeout(() => {
      void context.close().catch(() => {});
    }, 1000);
    const oscillator = context.createOscillator();
    const gain = context.createGain();
    oscillator.frequency.value = 660;
    gain.gain.value = 0;
    oscillator.connect(gain);
    gain.connect(context.destination);
    void context
      .resume()
      .then(() => {
        const now = context.currentTime;
        gain.gain.setValueAtTime(0, now);
        gain.gain.linearRampToValueAtTime(0.08, now + 0.015);
        gain.gain.exponentialRampToValueAtTime(0.001, now + 0.18);
        oscillator.start(now);
        oscillator.stop(now + 0.2);
        oscillator.onended = () => {
          window.clearTimeout(fallback);
          void context.close().catch(() => {});
        };
      })
      .catch(() => {
        void context.close().catch(() => {});
      });
  } catch {
    /* Notifications remain visible when audio is unavailable. */
  }
}
