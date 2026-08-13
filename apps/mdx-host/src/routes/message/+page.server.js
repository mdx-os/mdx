import { messageFirstViewport } from "../../lib/messageFirstViewport.js";

export function load({ locals }) {
  return {
    viewport: messageFirstViewport,
    session: locals.session
  };
}
