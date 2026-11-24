import type { SubmitFunction } from '@sveltejs/kit';
import { worded } from '$lib/platform-copy';
import type { WyrdProblem } from '$lib/server/problem';
import examples from '../../../../../wyrd-spec/schemas/ui_problem_examples.json';

/** The console could not reach Wyrd; the change stays pending to retry. */
const unreachable: WyrdProblem = { ...examples.upstream, remediation: 'Check your connection, then confirm again.' };

/** The destructive change waiting on the operator's fresh proof: what it is, and the form that applies it. */
export type PendingChange = { label: string; form: HTMLFormElement };

/**
 * Same-operator re-authentication before a destructive console change. Rust
 * refuses the change with `WYRD_AUTH_403_RECENT_LOGIN_REQUIRED` once the
 * session's five-minute proof is stale; this asks the signed-in operator to
 * prove it again, then submits that same form again. A proof finishing after
 * Cancel, or for a change since replaced, applies nothing.
 */
export class ConsoleReauth {
  pending = $state.raw<PendingChange | null>(null);
  issue = $state.raw<WyrdProblem | null>(null);
  confirming = $state(false);

  /** An `enhance` callback for a destructive form labelled `label`: a stale-proof refusal asks; every other result updates the page. */
  guard(label: string): SubmitFunction {
    return ({ formElement }) =>
      async ({ result, update }) => {
        const problem = result.type === 'failure' ? (result.data?.problem as WyrdProblem | undefined) : undefined;
        if (problem?.code === 'WYRD_AUTH_403_RECENT_LOGIN_REQUIRED') {
          this.pending = { label, form: formElement };
          this.issue = null;
          this.confirming = false;
          return;
        }
        await update({ reset: false });
      };
  }

  /** Close the prompt; a confirmation still in flight then applies nothing. */
  cancel(): void {
    this.pending = null;
    this.confirming = false;
  }

  /** Prove this session's own password again, then apply the change still pending. */
  async password(password: string): Promise<void> {
    const change = this.pending;
    this.confirming = true;
    this.issue = null;
    let problem: WyrdProblem | null = null;
    try {
      const response = await fetch('/platform/ui/reauth', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ password })
      });
      if (response.ok) return this.proven(change);
      problem = worded((await response.json()) as WyrdProblem, 'reauth', 401);
    } catch {
      problem = { ...unreachable };
    }
    // Only the prompt still asking takes the refusal, and it stays open to retry.
    if (this.pending !== change) return;
    this.confirming = false;
    this.issue = problem;
  }

  /** Apply `change` only while it is still the one asked about. */
  proven(change: PendingChange | null): void {
    if (!change || this.pending !== change) return;
    this.cancel();
    change.form.requestSubmit();
  }
}
