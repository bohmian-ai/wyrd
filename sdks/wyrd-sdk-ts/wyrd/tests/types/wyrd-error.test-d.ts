// Compile-only: `mise run ts:typecheck` fails when an assertion here breaks.
import { expectTypeOf } from "vitest";

import type { WyrdError, WyrdErrorCode } from "@wyrd/sdk";

// A caller switches on the generated catalog, never on an arbitrary string.
expectTypeOf<WyrdError["code"]>().toEqualTypeOf<WyrdErrorCode>();
expectTypeOf<"WYRD_VALA_404_RUNNING_QUERY_NOT_FOUND">().toExtend<WyrdErrorCode>();
expectTypeOf<"WYRD_NOT_A_REAL_CODE">().not.toExtend<WyrdErrorCode>();
