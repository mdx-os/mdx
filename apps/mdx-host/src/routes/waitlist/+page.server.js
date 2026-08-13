// The waitlist records two things, in two places, on purpose:
//
//   1. Contact identity (name + email) goes to the provider project's
//      beta_signups table - that is what lets a human actually send the
//      invite. It never enters the MDx kernel.
//   2. A governed waitlist receipt goes to the kernel with an email HASH
//      only, so the record of "someone asked" is durable and provable
//      without the kernel ever holding personal contact data.
//
// A duplicate signup is a success, not an error: the person is on the list.
import { createHash } from "node:crypto";
import { fail } from "@sveltejs/kit";
import { publicFunnelEnabled } from "../../lib/experienceProfile.server.js";

const ROLES = new Set(["engineer", "eng_lead", "founder", "other"]);

function clean(value, max) {
  return String(value ?? "")
    .trim()
    .slice(0, max);
}

export function load() {
  return { funnel: publicFunnelEnabled() };
}

export const actions = {
  default: async ({ request, fetch, locals }) => {
    const form = await request.formData();
    const name = clean(form.get("name"), 120);
    const email = clean(form.get("email"), 254).toLowerCase();
    const role = ROLES.has(String(form.get("role"))) ? String(form.get("role")) : "engineer";
    const goal = clean(form.get("goal"), 280);
    const referral = clean(form.get("referral"), 160);
    const consent = form.get("consent") === "on";

    if (!name) return fail(400, { error: "Tell us your name so the invite isn't addressed to a hash.", values: { name, email, role, goal, referral } });
    if (!/^[^@\s]+@[^@\s]+\.[^@\s]+$/.test(email)) {
      return fail(400, { error: "That email doesn't look complete - check it and try again.", values: { name, email, role, goal, referral } });
    }
    if (!consent) {
      return fail(400, { error: "We can only invite you if you're OK with us emailing you.", values: { name, email, role, goal, referral } });
    }

    const hash = createHash("sha256").update(email).digest("hex");

    // Contact store (provider-side). Insert-only for the public; a duplicate
    // email means they're already on the list, which is a success.
    let contactStored = false;
    if (locals.supabase) {
      const { error } = await locals.supabase.from("beta_signups").insert({
        full_name: name,
        email,
        email_hash: `sha256:${hash}`,
        role,
        build_goal: goal,
        referral_source: referral,
        consent: true
      });
      contactStored = !error || error.code === "23505";
    }

    // Governed receipt (kernel-side, hash only). Best-effort: the ask is on
    // the record even if the contact store is briefly unavailable.
    let receiptId = "";
    try {
      const response = await fetch("/api/kernel/beta/waitlist.json", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({
          applicant_id: `applicant_${hash.slice(0, 16)}`,
          email_hash: `sha256:${hash}`,
          role,
          repo_ecosystem: "unspecified",
          build_goal: goal || "evaluate_mdx",
          requested_cohort: "waitlist",
          referral_source: referral || "direct",
          follow_up_contact_consent: true
        })
      });
      const packet = await response.json().catch(() => null);
      receiptId = String(packet?.waitlist_receipt_id ?? "");
    } catch (error) {
      receiptId = "";
    }

    if (!contactStored) {
      return fail(502, {
        error: "Your contact details didn't save, so we cannot send an invite yet. Try again in a moment.",
        values: { name, email, role, goal, referral }
      });
    }

    return { success: true, firstName: name.split(" ")[0], receiptId };
  }
};
