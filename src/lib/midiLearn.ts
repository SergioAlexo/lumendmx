import type { MidiEvent, MidiTrigger } from "./showfile/generated";
import * as timing from "./timing";

const POLL_MS = 100;
const TIMEOUT_MS = 15_000;

/** Waits for the next MIDI message that `accept` takes, from the controller
 * ports. Resolves to null on timeout or when `signal` aborts. Any message
 * after the call counts, even from the control touched last. */
export async function learnMidi(accept: (e: MidiEvent) => boolean, signal?: AbortSignal): Promise<MidiEvent | null> {
  const logged = await timing.midiEvents(0).catch(() => []);
  let after = logged.length > 0 ? logged[logged.length - 1].seq : 0;
  const started = Date.now();
  while (!signal?.aborted && Date.now() - started < TIMEOUT_MS) {
    const events = await timing.midiEvents(after).catch(() => []);
    if (events.length > 0) after = events[events.length - 1].seq;
    const hit = events.find(accept);
    if (hit) return hit;
    await new Promise((r) => setTimeout(r, POLL_MS));
  }
  return null;
}

/** A pad or button press: a note on, or a CC. */
export const isPress = (e: MidiEvent) => e.kind === "noteOn" || e.kind === "cc";

export function toTrigger(e: MidiEvent): MidiTrigger {
  return { message: e.kind === "cc" ? "cc" : "note", channel: e.channel, number: e.number };
}

/** "Note 36 · ch 10", "CC 20 · any ch". */
export function describeTrigger(t: MidiTrigger): string {
  return `${t.message === "cc" ? "CC" : "Note"} ${t.number} · ${t.channel === 0 ? "any ch" : `ch ${t.channel}`}`;
}
