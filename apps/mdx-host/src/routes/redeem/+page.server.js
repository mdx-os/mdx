import { fail } from "@sveltejs/kit";

function cleanInvite(value) {
  return String(value ?? "").trim().slice(0, 160);
}

export function load({ locals, url }) {
  return {
    authenticated: locals.session?.authenticated === true,
    invite: cleanInvite(url.searchParams.get("invite"))
  };
}

export const actions = {
  default: async ({ request, fetch, locals }) => {
    const form = await request.formData();
    const inviteId = cleanInvite(form.get("invite_id"));
    if (!locals.session?.authenticated) {
      return fail(401, {
        error: "Sign in with the invited account before redeeming.",
        values: { invite: inviteId }
      });
    }
    if (!inviteId) {
      return fail(400, { error: "Paste the invite id from your email.", values: { invite: "" } });
    }

    const response = await fetch("/api/kernel/auth/invite-redemptions.json", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({
        redemption_id: `redeem_${inviteId.slice(0, 24)}_${Date.now().toString(36)}`,
        invite_id: inviteId,
        cohort_id: "cohort_beta",
        risk_tier: "standard",
        expected_first_task: "request_forge_recipe",
        use_case: "forge",
        support_owner: "beta_lead"
      })
    });
    const packet = await response.json().catch(() => ({}));
    const receiptId = String(packet?.invite_redemption_receipt_id ?? "");
    if (!response.ok || !receiptId) {
      return fail(response.status >= 400 && response.status < 500 ? response.status : 502, {
        error: String(packet?.reason ?? "That invite did not redeem. Check the id and try again."),
        values: { invite: inviteId }
      });
    }
    return { success: true, receiptId };
  }
};
