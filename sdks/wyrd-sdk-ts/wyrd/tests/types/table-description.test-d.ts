// Compile-only: `mise run ts:typecheck` fails when an assertion here breaks.
import { expectTypeOf } from "vitest";

import type { TableDescription } from "wyrd-ai";

type DataType = TableDescription["user_fields"][number]["data_type"];

// Recursive field types stay typed instead of collapsing to `unknown`.
expectTypeOf<Extract<DataType, { List: unknown }>["List"]["data_type"]>().toEqualTypeOf<DataType>();
expectTypeOf<Extract<DataType, { Struct: unknown }>["Struct"][number]["name"]>().toEqualTypeOf<string>();
expectTypeOf<TableDescription["physical_layout"]["sort_keys"][number]["column"]>().toEqualTypeOf<string>();
// Fields Rust skips when unset are optional and never null.
expectTypeOf<TableDescription["canonical_physical_fingerprint"]>().toEqualTypeOf<string | undefined>();
expectTypeOf<TableDescription["physical_layout"]["time_column"]>().toEqualTypeOf<string | undefined>();
expectTypeOf<TableDescription["user_fields"][number]["metadata"]>().toEqualTypeOf<
  Readonly<Record<string, string>> | undefined
>();
// An unskipped Option inside a variant is always present.
expectTypeOf<Extract<DataType, { Timestamp: unknown }>["Timestamp"]["tz"]>().toEqualTypeOf<string | null>();
