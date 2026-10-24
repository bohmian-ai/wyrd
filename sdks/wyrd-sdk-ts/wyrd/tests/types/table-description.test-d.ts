// Compile-only: `mise run ts:typecheck` fails when an assertion here breaks.
import { expectTypeOf } from "vitest";

import type { TableAlteration, TableDescription } from "wyrd-ai";

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
// A skipped Option inside a variant is optional and never null.
expectTypeOf<Extract<DataType, { Timestamp: unknown }>["Timestamp"]["tz"]>().toEqualTypeOf<string | undefined>();
// The version is an exact integer; only the wire spells it as a string.
expectTypeOf<TableDescription["table_version"]>().toEqualTypeOf<bigint>();
// An added column may omit `nullable`, which the server defaults to true, while
// a description always carries it.
expectTypeOf<{ name: "note"; data_type: "Utf8" }>().toMatchTypeOf<
  NonNullable<TableAlteration["add_columns"]>[number]
>();
expectTypeOf<TableDescription["user_fields"][number]["nullable"]>().toEqualTypeOf<boolean>();
