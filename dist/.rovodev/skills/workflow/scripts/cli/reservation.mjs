import { main as runReservationCommand } from "../pulse_reservations.mjs";

export * from "../pulse_reservations.mjs";

export function main(argv = process.argv.slice(2), context = {}) {
  return runReservationCommand(argv, context);
}
