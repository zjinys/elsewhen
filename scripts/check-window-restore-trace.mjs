import { readFileSync } from 'node:fs';

const phases = [];
let phase;
for (const line of readFileSync(process.argv[2], 'utf8').split('\n')) {
  const shown = line.match(/^PROBE show=(\d+)$/);
  if (shown) {
    phase = { show: Number(shown[1]), surface: null, attached: false };
    phases.push(phase);
  }
  if (line.startsWith('PROBE hide=')) phase = undefined;
  if (!phase) continue;
  const subsurface = line.match(/get_subsurface\(new id wl_subsurface#\d+, wl_surface#(\d+),/);
  if (subsurface) phase.surface = subsurface[1];
  if (phase.surface && line.includes(`wl_surface#${phase.surface}.attach(wl_buffer#`)) {
    phase.attached = true;
  }
}
for (const entry of phases) {
  console.log(`show=${entry.show}: ${entry.attached ? 'frame attached' : 'MISSING subsurface/frame'}`);
}
if (phases.length !== 6 || phases.some((entry, index) => entry.show !== index || !entry.attached)) {
  process.exitCode = 1;
}
