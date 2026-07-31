import { tableFromArrays, tableToIPC } from "apache-arrow";
import { describe, expect, it } from "vitest";

import {
  BifrostClient,
  BifrostQueryStream,
  IncompleteQueryStreamError,
  WyrdError,
} from "../../src/index.js";

describe("BifrostQueryStream", () => {
  it("yields Apache Arrow batches and retains the validated terminal", async () => {
    const ipc = tableToIPC(tableFromArrays({ value: [1, 2] }), "stream");
    const terminal = {
      outcome: "success",
      freshness: "fresh",
      row_count: 2,
      warnings: [],
      source_completion: [],
      error: null,
    };
    let index = 0;
    const native = {
      async next() {
        index += 1;
        return index === 1
          ? { ipc: Buffer.from(ipc), terminalJson: undefined }
          : { ipc: undefined, terminalJson: JSON.stringify(terminal) };
      },
      async close() {},
      terminalJson: null,
    };
    const stream = new BifrostQueryStream(native);
    const batches = [];
    for await (const batch of stream) {
      batches.push(batch);
    }
    expect(batches).toHaveLength(1);
    expect(batches[0]?.numRows).toBe(2);
    expect(stream.terminal).toEqual(terminal);
  });

  it("closes the native response when iteration stops early", async () => {
    let closed = false;
    const native = {
      async next() {
        return { ipc: undefined, terminalJson: undefined };
      },
      async close() {
        closed = true;
      },
      terminalJson: null,
    };
    const stream = new BifrostQueryStream(native);
    await stream.return();
    expect(closed).toBe(true);
  });

  it("raises a structured incomplete-stream error without parsing messages", async () => {
    const native = {
      async next() {
        return {
          ipc: undefined,
          terminalJson: undefined,
          errorCode: "WYRD_VALA_502_QUERY_STREAM_INCOMPLETE",
          errorStatus: 502,
          errorTitle: "Query stream incomplete",
          errorDetail: "response ended before terminal",
        };
      },
      async close() {},
      terminalJson: null,
    };
    const stream = new BifrostQueryStream(native);
    await expect(stream.next()).rejects.toBeInstanceOf(
      IncompleteQueryStreamError,
    );
  });
});

describe("BifrostClient", () => {
  it.each([
    {
      boundary: "Gate",
      code: "WYRD_PERMISSION_403_DENIED_RBAC",
      status: 403,
      title: "Permission denied (RBAC)",
      detail: "principal lacks bifrost_query:read",
      remediation: "Request the required role from a workspace admin.",
    },
    {
      boundary: "authentication",
      code: "WYRD_PERMISSION_401_UNAUTHENTICATED",
      status: 401,
      title: "Authentication required",
      detail: "access token is invalid",
      remediation:
        "Send a valid `Authorization: Bearer <token>` header before invoking permission-protected routes.",
    },
    {
      boundary: "transport",
      code: "WYRD_SERVER_503_SERVICE_UNAVAILABLE",
      status: 503,
      title: "Service unavailable",
      detail: "query transport is unavailable",
      remediation:
        "Retry with exponential backoff; the server is shedding load to protect inflight requests.",
    },
    {
      boundary: "request",
      code: "WYRD_VALA_400_QUERY_INVALID_SQL",
      status: 400,
      title: "Invalid or unsupported query SQL",
      detail: "only SELECT statements are accepted",
      remediation:
        "Submit a single SELECT statement; DDL/DML and unsupported constructs are rejected.",
    },
  ])(
    "preserves structured $boundary startup errors",
    async ({ code, status, title, detail, remediation }) => {
      const native = {
        async query() {
          return {
            takeStream() {
              return undefined;
            },
            errorCode: code,
            errorStatus: status,
            errorTitle: title,
            errorDetail: detail,
            errorRemediation: remediation,
          };
        },
      };
      const client = new BifrostClient(native as never);
      let error: unknown;
      try {
        await client.query({ sql: "SELECT 1" });
      } catch (reason) {
        error = reason;
      }

      expect(error).toBeInstanceOf(WyrdError);
      expect(error).toMatchObject({
        code,
        status,
        title,
        detail,
        remediation,
      });
    },
  );
});
