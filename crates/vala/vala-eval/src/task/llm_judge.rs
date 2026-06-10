//! LlmJudge executor - resolves deps, builds prompt context, calls
//! [`crate::judge::JudgeInvoker`] with `LlmJudge.max_retries`.
//!
//! Body lands in Commit 9 (`10-judge-invoker-and-media.md`).
