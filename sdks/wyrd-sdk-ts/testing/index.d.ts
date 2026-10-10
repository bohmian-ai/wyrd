import type {
  CardRef,
  HydrationSummary,
  ProviderCredentialView,
  RegistrationReceipt,
  WyrdClient,
} from "@wyrd/sdk";

export { NativeWyrdTestServer, type NativeTestServerOptions, startTestServer } from "./native.cjs";

/** One loader diagnostic reported by {@link cli.plan}. */
export interface PlanDiagnostic {
  readonly code: string;
  readonly status: number;
  readonly severity: string;
  readonly path: string;
  readonly span?: unknown;
  readonly message: string;
  readonly remediation: string;
  readonly details?: unknown;
}

/** Deterministic local registration plan printed by `wyrd plan --format json`. */
export interface PlanReport {
  readonly ok: boolean;
  readonly cards: readonly {
    readonly kind: string;
    readonly space: string | null;
    readonly name: string;
    readonly version: string | null;
  }[];
  readonly diagnostics: readonly PlanDiagnostic[];
}

/** Result of `wyrd load --format json`. */
export interface LoadOutput {
  readonly card_ref: CardRef;
  readonly materialized: boolean;
}

/** Card selector: a `uid` with `kind`, or `kind`, `space`, and `name`; `version` narrows either. */
export interface CliCardSelector {
  readonly kind?: string;
  readonly space?: string;
  readonly name?: string;
  readonly version?: string;
  readonly uid?: string;
}

/** Response of `wyrd auth issue-key`; `key` is the plaintext API key, returned exactly once. */
export interface IssueKeyResponse {
  readonly key_id: string;
  /** The Card's Service or Agent principal, the id its Role assignments are addressed by. */
  readonly principal_id: string;
  readonly key: string;
  readonly prefix: string;
  readonly card_ref: CardRef;
  readonly created_at: string;
  readonly expires_at: string;
}

/** Provider credential submission for {@link cli.putProviderCredential}. */
export interface ProviderCredentialWrite {
  readonly name: string;
  readonly provider: string;
  readonly source:
    | { readonly environment: { readonly binding: string } }
    | { readonly external_secret: { readonly backend: string; readonly reference: string } }
    | { readonly managed_secret: { readonly secret: string } };
}

/** What {@link cli.mcpInstall} did to one host's configuration file. */
export interface McpHostReport {
  /** The `--host` value naming the host, such as `claude-code`. */
  readonly host: "codex" | "claude-code" | "copilot-cli" | "vscode" | "cursor" | "pi" | "hermes";
  /** Connected: `added`, `updated`, or `unchanged`; otherwise why the file was left as it was. */
  readonly status: "added" | "updated" | "unchanged" | "not_detected" | "conflict" | "unreadable" | "unwritable";
  /** The host's configuration file, or `null` when the host was not detected. */
  readonly path: string | null;
  /** Why the host was not connected and what to do next, or `null`. */
  readonly detail: string | null;
}

/** Per-host outcomes returned by {@link cli.mcpInstall}, in selection order. */
export interface McpInstallReport {
  readonly hosts: readonly McpHostReport[];
}

/** The client a networked {@link cli} command runs as. */
export interface CliClientOptions {
  /** Client the command runs as; omitted, the ambient credential chain resolves one. */
  readonly client?: WyrdClient;
}

/**
 * The `wyrd` CLI in process, for tests only: the same Rust command
 * implementation the installed `wyrd` executable runs.
 *
 * Each command returns the value it prints with `--format json` and throws a
 * `WyrdError` instead of exiting.
 */
export declare const cli: {
  /** Validate a local Card tree without contacting a server (`wyrd plan`). */
  plan(path: string): PlanReport;
  /** Register a local Card tree and return its receipt (`wyrd apply`). */
  apply(path: string, options?: CliClientOptions): Promise<RegistrationReceipt>;
  /** Hydrate a Card's reachable graph into `outputDir` (`wyrd get`). */
  get(
    selector: CliCardSelector,
    outputDir: string,
    options?: CliClientOptions & { readonly metadataOnly?: boolean },
  ): Promise<HydrationSummary>;
  /** Load one Card and materialize its artifacts (`wyrd load`). */
  load(selector: CliCardSelector, options?: CliClientOptions & { readonly path?: string }): Promise<LoadOutput>;
  /** Issue an API key bound to one exact Card (`wyrd auth issue-key`). */
  issueKey(
    card: {
      readonly kind: string;
      readonly name: string;
      readonly version: string;
      readonly space: string;
      readonly label?: string;
      readonly expiresInSeconds?: number;
    },
    options?: CliClientOptions,
  ): Promise<IssueKeyResponse>;
  /** Create or rotate a provider credential; a rejected body is not quoted and the view is redacted. */
  putProviderCredential(write: ProviderCredentialWrite, options?: CliClientOptions): Promise<ProviderCredentialView>;
  /** Terminally revoke a provider credential (`wyrd gateway credential revoke`). */
  revokeProviderCredential(name: string, options?: CliClientOptions): Promise<ProviderCredentialView>;
  /** Delete an unreferenced provider credential; an absent name succeeds. */
  deleteProviderCredential(name: string, options?: CliClientOptions): Promise<void>;
  /** Connect the named MCP hosts to Wyrd (`wyrd mcp install --host ...`); per-host failures are reported, not thrown. */
  mcpInstall(hosts: readonly string[], options?: { readonly server?: string }): McpInstallReport;
};
