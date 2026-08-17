"""Fixture tests for Wyrd v3 planning artifact validation."""
from __future__ import annotations
import importlib.util, json, subprocess, tempfile, unittest
from pathlib import Path
SCRIPT=Path(__file__).with_name("validate_v3_artifacts.py"); SPEC=importlib.util.spec_from_file_location("validator",SCRIPT); assert SPEC and SPEC.loader
validator=importlib.util.module_from_spec(SPEC); SPEC.loader.exec_module(validator)

class Fixtures(unittest.TestCase):
    """Cover source identity, traceability, locks, proof, rehearsal, and DAG gates."""
    def setUp(self)->None:
        """Create an isolated accepted Git source and canonical draft plan."""
        self.tmp=tempfile.TemporaryDirectory(); base=Path(self.tmp.name); self.repo=base/"repo"; self.plan=base/"plan"; (self.repo/"src").mkdir(parents=True); (self.plan/"tasks").mkdir(parents=True); (self.plan/"evidence").mkdir()
        (self.repo/"src/lib.rs").write_text("struct Owner;\nimpl Owner { fn old() {} }\n")
        subprocess.run(["git","init","-q"],cwd=self.repo,check=True); subprocess.run(["git","config","user.name","Fixture"],cwd=self.repo,check=True); subprocess.run(["git","config","user.email","fixture@example.com"],cwd=self.repo,check=True); subprocess.run(["git","remote","add","origin","https://example.com/wyrd.git"],cwd=self.repo,check=True); subprocess.run(["git","add","."],cwd=self.repo,check=True); subprocess.run(["git","commit","-qm","fixture"],cwd=self.repo,check=True)
        self.rev=subprocess.check_output(["git","rev-parse","HEAD"],cwd=self.repo,text=True).strip()
        self.plan_text=f"# Plan\nStatus: Draft\nRepository revision: {self.rev}\n\n"+"\n\n".join(f"## {h}\n\n"+("- R1. Result.\n- D1. Owner.\n- AC1. Proof." if h=="Requirements" else "R1 D1 AC1 exact repository evidence and bounded outcome.") for h in validator.PLAN_HEADINGS)
        def task_body(h:str)->str:
            if h=="Target paths, symbols, callers, and consumers": return "| Path | Path kind | Symbol | Symbol kind | Why | Callers | Consumers |\n|---|---|---|---|---|---|---|\n| `src/lib.rs` | existing | `Owner::run` | new | add locked owner operation | `tests::owner_runs` | `Owner` |"
            if h=="Required types and interfaces": return "```rust\nfn run(&self) -> Result<(), Error>\n```"
            if h=="Failure and edge-case matrix": return "| State | Trigger | Exact result |\n|---|---|---|\n| rejected | invalid input | return typed Error without mutation |"
            values={"User and operator value":"The developer receives a typed deterministic result.","Current behavior and evidence":"Evidence: `src/lib.rs:Owner` has no run method.","Non-goals":"Do not change unrelated runtime behavior.","Allowed scope":"Only `src/lib.rs` and its inline tests.","Prohibited changes":"Must not alter public wire contracts.","Control flow and pseudocode":"1. Validate input.\n2. Return typed result.","Allocation and lifecycle contract":"Construction owns no heap allocation; shutdown, cancellation, concurrency, and recovery are synchronous no-ops.","Required tests":"Add `tests::owner_runs` with exact result assertions.","Required features":"Default features only.","Focused verification":"Run `mise exec -- cargo test --locked -p owner --lib --no-run`.","Concurrency and integration locks":"Use the contracts lock and serialize the Cargo lane.","Commands explicitly excluded":"Exclude `mise run pre-pr`.","Stop and escalate if":"Stop if the owner contract differs at the accepted revision.","Completion evidence":"Record command result, test output, digest, and diff."}
            if h in values:return values[h]
            return "R1 D1 AC1 exact behavior, repository boundary, and named proof."
        self.task_text=f"# T1\nStatus: Planned\nRepository revision: {self.rev}\nDepends on: None\n\n"+"\n\n".join(f"## {h}\n\n{task_body(h)}" for h in validator.TASK_HEADINGS)
        (self.plan/"plan.md").write_text(self.plan_text); (self.plan/"tasks/01-owner.md").write_text(self.task_text)
        command="mise exec -- cargo test --locked -p owner --lib --no-run"; evidence={"command":command,"exit_code":0,"selected_count":0,"output":"compiled test target"}; ep=self.plan/"evidence/T1-AC1.json"; ep.write_text(json.dumps(evidence))
        self.data={"schema_version":3,"plan_id":"p","repository":{"origin":"https://example.com/wyrd.git","revision":self.rev},"artifacts":{},"delegation":{"model":"gpt-5.6-sol","reasoning_effort":"low","cargo_execution_lanes":1},"requirements":{"R1":["T1"]},"decisions":["D1"],"locks":[{"category":"contracts","key":"Owner::run","owners":["T1"]}],"tasks":[{"id":"T1","packet":"tasks/01-owner.md","depends_on":[],"write_set":[{"path":"src/lib.rs","kind":"existing","why":"add locked owner operation","symbols":[{"name":"Owner::run","kind":"new"}]}],"requirements":["R1"],"decisions":["D1"],"acceptance":["AC1"],"lifecycle":{d:{"applicable":d=="construction","behavior":"canonical packet names exact behavior and boundary","proof":"AC1"} for d in validator.LIFECYCLE},"proofs":[{"acceptance":"AC1","package":"owner","target":"lib","features":[],"selector":"tests::owner_runs","test_kind":"new","test_path":"src/lib.rs","test_name":"tests::owner_runs","expected_selected_count":1,"setup":"none","lane":"cargo-1","expected_result":"one test passes","command":command,"preflight":{"path":"evidence/T1-AC1.json","digest":validator.sha256(ep)}}]}]}
        self.write()
    def tearDown(self)->None:
        """Remove fixture storage."""
        self.tmp.cleanup()
    def write(self)->None:
        """Refresh canonical artifact digests and manifest."""
        self.data["artifacts"]={"plan.md":validator.sha256(self.plan/"plan.md"),"tasks/01-owner.md":validator.sha256(self.plan/"tasks/01-owner.md")}; (self.plan/"execution-manifest.json").write_text(json.dumps(self.data))
    def errors(self)->list[str]:
        """Validate the current fixture."""
        return validator.validate(self.plan,self.repo)
    def assert_error(self,needle:str)->None:
        """Require one diagnostic fragment."""
        self.assertTrue(any(needle in e for e in self.errors()),self.errors())
    def test_valid_draft(self)->None:
        """Accept a source-bound, traceable draft."""
        self.assertEqual([],self.errors())

    def test_path_overlap_includes_ancestry(self)->None:
        """Treat a directory and its descendant as overlapping ownership."""
        self.assertTrue(validator.overlaps("src", "src/lib.rs")); self.assertFalse(validator.overlaps("src/a.rs", "src/b.rs"))
    def test_source_origin_revision_path_symbol_and_ancestry(self)->None:
        """Reject false repository, stale source identity, and unsafe paths."""
        self.data["repository"]["origin"]="bad"; self.data["repository"]["revision"]="abc"; self.data["tasks"][0]["write_set"][0].update(path="../escape",kind="existing"); self.write(); self.assert_error("40-hex"); self.assert_error("origin"); self.assert_error("write path")
    def test_metadata_ids_and_write_cross_checks(self)->None:
        """Reject canonical/manifest status, ID, dependency, and write drift."""
        self.data["requirements"]={"R2":["T1"]}; self.data["tasks"][0]["depends_on"]=["T2"]; self.data["tasks"][0]["write_set"][0]["path"]="src/other.rs"; self.write(); self.assert_error("requirements are not"); self.assert_error("dependency mismatch"); self.assert_error("target path/symbol/why mismatch")
    def test_locks_lifecycle_and_preflight_evidence(self)->None:
        """Reject invalid lock categories, missing dimensions, and zero/unbound proof."""
        self.data["locks"][0]["category"]="other"; self.data["tasks"][0]["lifecycle"].pop("recovery"); ep=self.plan/"evidence/T1-AC1.json"; payload=json.loads(ep.read_text()); payload["output"]="changed evidence"; ep.write_text(json.dumps(payload)); self.write(); self.assert_error("invalid lock"); self.assert_error("lifecycle dimensions"); self.assert_error("evidence digest invalid")
    def test_dag_overlap_cycle_and_lock_owner_order(self)->None:
        """Reject cycles and overlapping unordered ownership."""
        peer=json.loads(json.dumps(self.data["tasks"][0])); peer.update(id="T2",packet="tasks/02-peer.md",depends_on=["T1"]); (self.plan/"tasks/02-peer.md").write_text(self.task_text.replace("# T1","# T2").replace("Depends on: None","Depends on: T1")); self.data["tasks"][0]["depends_on"]=["T2"]; (self.plan/"tasks/01-owner.md").write_text(self.task_text.replace("Depends on: None","Depends on: T2")); self.data["tasks"].append(peer); self.data["requirements"]["R1"].append("T2"); self.write(); self.assert_error("DAG cycle")
    def test_approved_requires_digest_bound_pass(self)->None:
        """Reject Ready/Approved state without a current external rehearsal record."""
        (self.plan/"plan.md").write_text(self.plan_text.replace("Status: Draft","Status: Approved")); (self.plan/"tasks/01-owner.md").write_text(self.task_text.replace("Status: Planned","Status: Ready")); self.write(); self.assert_error("require rehearsal")
        record={"verdict":"PASS","manifest_digest":validator.sha256(self.plan/"execution-manifest.json"),"artifact_digests":self.data["artifacts"],"repository":self.data["repository"]}; (self.plan/"rehearsal.json").write_text(json.dumps(record)); self.assertEqual([],self.errors())

    def test_one_to_one_proof_and_task_owned_lifecycle(self)->None:
        """Reject duplicate acceptance proof and foreign lifecycle proof IDs."""
        self.data["tasks"][0]["proofs"].append(json.loads(json.dumps(self.data["tasks"][0]["proofs"][0]))); self.data["tasks"][0]["lifecycle"]["shutdown"]["proof"]="AC9"; self.write(); self.assert_error("one-to-one"); self.assert_error("another task")

    def test_derived_manifest_lock_is_required(self)->None:
        """Reject omitted shared-artifact ownership inferred from write paths."""
        old=self.rev; (self.repo/"Cargo.toml").write_text("[workspace]\n"); subprocess.run(["git","add","Cargo.toml"],cwd=self.repo,check=True); subprocess.run(["git","commit","-qm","manifest"],cwd=self.repo,check=True); self.rev=subprocess.check_output(["git","rev-parse","HEAD"],cwd=self.repo,text=True).strip(); self.data["repository"]["revision"]=self.rev
        (self.plan/"plan.md").write_text(self.plan_text.replace(old,self.rev)); (self.plan/"tasks/01-owner.md").write_text(self.task_text.replace(old,self.rev).replace("`src/lib.rs`","`Cargo.toml`").replace("`Owner::run`","`workspace`").replace("new | add locked owner operation","existing | update workspace manifest")); self.data["tasks"][0]["write_set"]=[{"path":"Cargo.toml","kind":"existing","why":"update workspace manifest","symbols":[{"name":"workspace","kind":"existing"}]}]; self.write(); self.assert_error("omitted derived manifests lock")

    def test_rejects_skeletal_t6_packet(self)->None:
        """Reject a heading-complete packet whose content remains generic and ownerless."""
        skeletal=f"# T1\nStatus: Planned\nRepository revision: {self.rev}\nDepends on: None\n\n"+"\n\n".join(f"## {h}\n\nadd a concrete owner and proof" for h in validator.TASK_HEADINGS)
        (self.plan/"tasks/01-owner.md").write_text(skeletal); self.write(); self.assert_error("section is empty or generic"); self.assert_error("target path/symbol/why mismatch")

if __name__=="__main__": unittest.main()
