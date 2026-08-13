// Evidence view: the first UI that actually renders a receipt. Receipts are
// the product's currency, and until now every receipt id in the app was dead
// text - GET /receipts/{id} answers live and nothing read it.
import { error } from "@sveltejs/kit";

export async function load({ fetch, params }) {
  const receiptId = String(params.receipt ?? "").trim();
  if (!/^[a-z0-9_\-]+$/i.test(receiptId)) {
    throw error(404, "That receipt id does not look right.");
  }
  let receipt = null;
  try {
    const response = await fetch(`/api/kernel/receipts/${encodeURIComponent(receiptId)}`, {
      signal: AbortSignal.timeout(4000)
    });
    if (response.ok) receipt = await response.json();
  } catch (err) {
    receipt = null;
  }
  return { receiptId, receipt };
}
