// Evals: the suites are generated contract truth (the checks the company
// holds itself to, complete without the kernel). The verdicts are runtime
// truth - we read the install's own receipts through the proxy, fail-soft
// to null so the surface invites instead of inventing a passing report.
import { evalSuites, withVerdicts } from "../../lib/evalSuites.js";

async function readJson(fetchImpl, path) {
  try {
    const response = await fetchImpl(`/api/kernel${path}`, {
      signal: AbortSignal.timeout(1500)
    });
    return response.ok ? await response.json() : null;
  } catch (error) {
    return null;
  }
}

export async function load({ fetch }) {
  const receipts = await readJson(fetch, "/receipts.json");
  const receiptIds = Array.isArray(receipts?.receipt_ids) ? receipts.receipt_ids : null;

  const suites = withVerdicts(evalSuites(), receiptIds ?? []);
  const checkedCount = receiptIds ? suites.filter((suite) => suite.runCount > 0).length : 0;

  return {
    suites,
    // null means we could not read the install at all - the page invites.
    // 0 means we read it and there is honestly nothing run yet.
    verdictsKnown: receiptIds != null,
    checkedCount,
    totalChecks: suites.reduce((sum, suite) => sum + suite.checkCount, 0),
    boundary:
      "this page reads the registered gates and the run evidence they leave; suites run as their loops under policy, which are agent-run by doctrine, so nothing is started from here",
    safeNext:
      "run a loop in Forge or ask a companion in Twin to put real runs on the record, and the first verdict lands here"
  };
}
