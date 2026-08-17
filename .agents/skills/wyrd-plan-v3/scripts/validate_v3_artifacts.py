#!/usr/bin/env python3
"""Validate canonical Wyrd v3 plans, scheduling identity, and proof evidence."""
from __future__ import annotations
import argparse, hashlib, json, re, subprocess, sys
from pathlib import Path, PurePosixPath

DIGEST = re.compile(r"sha256:[0-9a-f]{64}\Z")
HEX40 = re.compile(r"[0-9a-f]{40}\Z")
IDS = {"R": re.compile(r"R[1-9][0-9]*\Z"), "D": re.compile(r"D[1-9][0-9]*\Z"), "T": re.compile(r"T[1-9][0-9]*\Z"), "AC": re.compile(r"AC[1-9][0-9]*\Z")}
LOCK_CATEGORIES = {"semantic", "contracts", "manifests", "migrations", "generated", "fixtures", "config"}
LIFECYCLE = {"construction", "shutdown", "cancellation", "concurrency", "recovery"}
PLAN_HEADINGS = ["Objective", "Current state and evidence", "Requirements", "Non-goals", "Constraints", "Concurrency design and alternatives", "Architecture and design decisions", "Domain and data contracts", "Interfaces and function contracts", "Control flow and pseudocode", "Failure and edge-case matrix", "Milestones", "Task inventory", "Global acceptance criteria", "Verification strategy", "Closeout verification", "Risks, migration, and rollout", "Execution handoff"]
TASK_HEADINGS = ["Objective", "User and operator value", "Current behavior and evidence", "Required changes", "Non-goals", "Allowed scope", "Prohibited changes", "Target paths, symbols, callers, and consumers", "Required types and interfaces", "Implementation guidance", "Control flow and pseudocode", "Failure and edge-case matrix", "Allocation and lifecycle contract", "Acceptance criteria", "Required tests", "Required features", "Focused verification", "Concurrency and integration locks", "Commands explicitly excluded", "Stop and escalate if", "Completion evidence"]
GENERIC = re.compile(r"\b(?:add a concrete|add recovery suite owner|add [^.\n]+ and proof|as needed|follow existing patterns|appropriate tests?|TBD|TODO)\b", re.I)
TASK_SEMANTICS = {
    "User and operator value": re.compile(r"\b(user|operator|agent|developer)\b",re.I),
    "Current behavior and evidence": re.compile(r"(`[^`]+`|evidence:|git show|CodeGraph)",re.I),
    "Non-goals": re.compile(r"\b(do not|out of scope|unchanged)\b",re.I),
    "Allowed scope": re.compile(r"`[^`/]+/[^`]+`"),
    "Prohibited changes": re.compile(r"\b(do not|must not|prohibited)\b",re.I),
    "Control flow and pseudocode": re.compile(r"(^\s*[1-9][.)]|\b(if|then|return|await|commit|rollback)\b)",re.I|re.M),
    "Allocation and lifecycle contract": re.compile(r"\b(allocation|construction|shutdown|cancellation|concurrency|recovery)\b",re.I),
    "Required tests": re.compile(r"(::|test_[a-z0-9_]+)",re.I),
    "Required features": re.compile(r"\b(default|feature|features)\b",re.I),
    "Focused verification": re.compile(r"`(?:mise|uv|pnpm) [^`]+`"),
    "Concurrency and integration locks": re.compile(r"\b(lock|serial|lane|dependency)\b",re.I),
    "Commands explicitly excluded": re.compile(r"`[^`]+`"),
    "Stop and escalate if": re.compile(r"\b(if|when)\b",re.I),
    "Completion evidence": re.compile(r"\b(command|digest|test|diff|result)\b",re.I),
}

def sha256(path: Path) -> str:
    """Return a manifest-form digest."""
    return "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()

def git(root: Path, *args: str) -> subprocess.CompletedProcess[str]:
    """Run one read-only Git query."""
    return subprocess.run(["git", *args], cwd=root, text=True, capture_output=True, check=False)

def headings(text: str) -> list[str]:
    """Extract level-two headings in order."""
    return re.findall(r"^## (.+?)\s*$", text, re.M)

def ids(text: str, prefix: str) -> set[str]:
    """Extract stable IDs from canonical prose."""
    return set(re.findall(rf"\b{prefix}[1-9][0-9]*\b", text))

def metadata(text: str, key: str) -> str:
    """Read one canonical top-level metadata value."""
    match = re.search(rf"^{re.escape(key)}:\s*(.+?)\s*$", text, re.M)
    return match.group(1) if match else ""

def sections(text: str) -> dict[str, str]:
    """Split canonical level-two sections without accepting reordered headings."""
    matches=list(re.finditer(r"^## (.+?)\s*$",text,re.M)); result={}
    for index,match in enumerate(matches):
        result[match.group(1)]=text[match.end():matches[index+1].start() if index+1<len(matches) else len(text)].strip()
    return result

def target_rows(body: str) -> list[dict[str,str]]:
    """Parse exact target ownership rows from the canonical seven-column table."""
    rows=[]
    for line in body.splitlines():
        if not line.startswith("|"): continue
        cells=[cell.strip().strip("`") for cell in line.strip().strip("|").split("|")]
        if len(cells)==7 and cells[0] not in {"Path","---"} and not set(cells[0])<={"-",":"}:
            rows.append(dict(zip(("path","path_kind","symbol","symbol_kind","why","callers","consumers"),cells)))
    return rows

def safe_path(value: str) -> bool:
    """Reject absolute, escaping, or non-normalized artifact paths."""
    path = PurePosixPath(value)
    return bool(value) and not path.is_absolute() and ".." not in path.parts and path.as_posix() == value

def overlaps(left: str, right: str) -> bool:
    """Return whether normalized paths are equal or ancestor/descendant."""
    a, b = PurePosixPath(left).parts, PurePosixPath(right).parts
    return a[:len(b)] == b or b[:len(a)] == a

def derived_lock(path: str) -> str | None:
    """Derive shared-artifact lock categories from repository write paths."""
    name = PurePosixPath(path).name.lower(); lowered = path.lower()
    if name in {"cargo.toml","cargo.lock","pyproject.toml","package.json","pnpm-lock.yaml","mise.toml"}: return "manifests"
    if "migration" in lowered: return "migrations"
    if any(token in lowered for token in ("generated","schema","openapi",".pyi")): return "generated"
    if any(token in lowered for token in ("fixture","fixtures","testdata")): return "fixtures"
    if any(token in lowered for token in ("config",".github/","dockerfile","compose")): return "config"
    return None

def graph_metrics(graph: dict[str, list[str]], slots: int = 3) -> dict[str, float | int] | None:
    """Compute deterministic unit-duration DAG metrics or return None for an invalid graph."""
    if not graph or slots < 1 or any(not isinstance(deps, list) for deps in graph.values()):
        return None
    if any(dep not in graph for deps in graph.values() for dep in deps):
        return None
    remaining = set(graph)
    complete: set[str] = set()
    waves = 0
    max_width = 0
    while remaining:
        ready = sorted(task for task in remaining if set(graph[task]) <= complete)
        if not ready:
            return None
        max_width = max(max_width, len(ready))
        selected = ready[:slots]
        complete.update(selected)
        remaining.difference_update(selected)
        waves += 1
    depth: dict[str, int] = {}
    pending = set(graph)
    while pending:
        progressed = False
        for task in sorted(pending):
            if all(dep in depth for dep in graph[task]):
                depth[task] = 1 + max((depth[dep] for dep in graph[task]), default=0)
                pending.remove(task)
                progressed = True
                break
        if not progressed:
            return None
    def ordered(task: str, predecessor: str, seen: set[str] | None = None) -> bool:
        visited = set() if seen is None else seen
        if task in visited:
            return False
        visited.add(task)
        return predecessor in graph[task] or any(ordered(dep, predecessor, visited) for dep in graph[task])
    parallel = {
        task for task in graph
        if any(peer != task and not ordered(task, peer) and not ordered(peer, task) for peer in graph)
    }
    return {
        "critical_path_tasks": max(depth.values()),
        "max_runnable_width": max_width,
        "scheduled_waves": waves,
        "average_occupied_slots": round(len(graph) / waves, 3),
        "parallel_task_fraction": round(len(parallel) / len(graph), 3),
    }

def audited_acceptance(command: str, selector: str) -> bool:
    """Accept direct lanes or the one repository-audited Postgres wrapper."""
    if command.startswith(("mise ","uv ","pnpm ")):
        return selector in command
    prefix="scripts/postgres/with-test-postgres.sh -- bash -lc "
    if not command.startswith(prefix) or selector not in command:
        return False
    payload=command[len(prefix):].strip()
    if len(payload)<2 or payload[0] not in "'\"" or payload[-1]!=payload[0]:
        return False
    inner=payload[1:-1]
    return inner.startswith("mise run db:migrate:inner && mise exec -- cargo test ")

def validate(plan_dir: Path, repo: Path) -> list[str]:
    """Return every artifact, source, traceability, scheduling, and proof error."""
    errors: list[str] = []
    mp, pp = plan_dir / "execution-manifest.json", plan_dir / "plan.md"
    if not mp.is_file() or not pp.is_file(): return ["missing plan.md or execution-manifest.json"]
    try: data = json.loads(mp.read_text())
    except Exception as exc: return [f"invalid manifest: {exc}"]
    if data.get("schema_version") != 3: errors.append("schema_version must equal 3")
    revision = str(data.get("repository", {}).get("revision", "")); origin = str(data.get("repository", {}).get("origin", ""))
    if not HEX40.fullmatch(revision): errors.append("repository revision must be exact 40-hex")
    elif git(repo, "cat-file", "-e", f"{revision}^{{commit}}").returncode: errors.append("repository commit does not exist")
    remote = git(repo, "remote", "get-url", "origin").stdout.strip()
    if not origin or origin != remote: errors.append("repository origin does not match remote origin")
    delegation = data.get("delegation", {})
    if delegation.get("model") != "gpt-5.6-sol" or delegation.get("reasoning_effort") != "low": errors.append("root and delegated roles must be gpt-5.6-sol low")
    lanes = delegation.get("cargo_execution_lanes")
    if not isinstance(lanes, int) or not 0 <= lanes <= 2: errors.append("cargo_execution_lanes must be 0, 1, or 2")
    plan_text = pp.read_text(); plan_status = metadata(plan_text, "Status")
    if headings(plan_text) != PLAN_HEADINGS: errors.append("plan headings must exactly match canonical order")
    plan_sections=sections(plan_text)
    for heading in PLAN_HEADINGS:
        body=plan_sections.get(heading,"")
        if not body or GENERIC.search(body): errors.append(f"plan section is empty or generic: {heading}")
    if metadata(plan_text, "Repository revision") != revision: errors.append("plan revision metadata mismatch")
    artifacts = data.get("artifacts", {})
    tasks = {t.get("id"): t for t in data.get("tasks", []) if isinstance(t, dict)}
    expected = {"plan.md", *(str(t.get("packet", "")) for t in tasks.values())}
    if set(artifacts) != expected or "rehearsal.json" in artifacts: errors.append("artifact set must contain only canonical plan and packets")
    for rel, digest in artifacts.items():
        path = plan_dir / rel
        if not safe_path(rel) or not DIGEST.fullmatch(str(digest)) or not path.is_file() or (path.is_file() and sha256(path) != digest): errors.append(f"STALE or invalid artifact: {rel}")
    plan_rs, plan_ds, plan_acs = ids(plan_text,"R"), ids(plan_text,"D"), ids(plan_text,"AC")
    manifest_rs, manifest_ds = set(data.get("requirements", {})), set(data.get("decisions", []))
    if plan_rs != manifest_rs: errors.append("requirements are not bidirectionally traceable")
    if plan_ds != manifest_ds: errors.append("decisions are not bidirectionally traceable")
    graph: dict[str,list[str]] = {}
    proof_prerequisites: dict[str,set[str]] = {}
    writes: dict[str,set[str]] = {}
    all_task_acs: set[str] = set()
    for tid, task in tasks.items():
        if not IDS["T"].fullmatch(str(tid)): errors.append(f"invalid task id {tid}")
        rel = str(task.get("packet", "")); packet = plan_dir / rel
        text = packet.read_text() if packet.is_file() else ""
        if headings(text) != TASK_HEADINGS: errors.append(f"{tid} headings must exactly match canonical order")
        task_sections=sections(text)
        for heading in TASK_HEADINGS:
            body=task_sections.get(heading,"")
            if not body or GENERIC.search(body): errors.append(f"{tid} section is empty or generic: {heading}")
            pattern=TASK_SEMANTICS.get(heading)
            if pattern and not pattern.search(body): errors.append(f"{tid} section lacks required semantic fields: {heading}")
        if metadata(text,"Repository revision") != revision: errors.append(f"{tid} revision metadata mismatch")
        if metadata(text,"Status") not in {"Planned","Ready","Complete","Blocked"}: errors.append(f"{tid} invalid status")
        deps = task.get("depends_on", []); graph[str(tid)] = deps if isinstance(deps,list) else []
        proof_prerequisites[str(tid)] = set()
        packet_deps = metadata(text,"Depends on"); packet_dep_set = set() if packet_deps == "None" else ids(packet_deps,"T")
        if set(graph[str(tid)]) != packet_dep_set: errors.append(f"{tid} dependency mismatch")
        for field,prefix in (("requirements","R"),("decisions","D"),("acceptance","AC")):
            refs=set(task.get(field,[])); packet_ids=ids(text,prefix)
            if refs != packet_ids: errors.append(f"{tid} {field} mismatch")
            if field=="requirements" and not refs <= manifest_rs: errors.append(f"{tid} references unknown requirement")
            if field=="decisions" and not refs <= manifest_ds: errors.append(f"{tid} references unknown decision")
            if field=="acceptance": all_task_acs |= refs
        lifecycle=task.get("lifecycle",{})
        if set(lifecycle)!=LIFECYCLE: errors.append(f"{tid} lifecycle dimensions incomplete")
        for dimension,entry in lifecycle.items():
            if not isinstance(entry,dict) or not isinstance(entry.get("applicable"),bool) or not str(entry.get("behavior","")).strip() or not IDS["AC"].fullmatch(str(entry.get("proof",""))): errors.append(f"{tid} invalid lifecycle proof {dimension}")
        task_writes:set[str]=set(); manifest_targets:set[tuple[str,str,str]]=set(); has_new_or_cross=False
        for item in task.get("write_set",[]):
            path=str(item.get("path","")); kind=item.get("kind"); why=str(item.get("why","")).strip()
            if not why or GENERIC.search(why): errors.append(f"{tid} write identity lacks exact why")
            coverage=item.get("coverage",[])
            if not isinstance(coverage,list) or not coverage or any(not str(value).strip() for value in coverage): errors.append(f"{tid} write identity lacks declared proof coverage")
            if not safe_path(path) or kind not in {"existing","new"}: errors.append(f"{tid} invalid write path identity")
            elif kind=="existing" and git(repo,"cat-file","-e",f"{revision}:{path}").returncode: errors.append(f"{tid} existing path absent at source commit: {path}")
            elif kind=="new" and git(repo,"cat-file","-e",f"{revision}:{PurePosixPath(path).parent}").returncode: errors.append(f"{tid} new path parent absent at source commit: {path}")
            source=git(repo,"show",f"{revision}:{path}").stdout if kind=="existing" else ""
            for symbol in item.get("symbols",[]):
                if symbol.get("kind") not in {"existing","new"} or (symbol.get("kind")=="existing" and str(symbol.get("name","")).split("::")[-1] not in source): errors.append(f"{tid} invalid or absent symbol identity")
                manifest_targets.add((path,str(symbol.get("name","")),why)); has_new_or_cross |= symbol.get("kind")=="new" or bool(symbol.get("cross_owner"))
            task_writes.add(path)
        writes[str(tid)]=task_writes
        rows=target_rows(task_sections.get("Target paths, symbols, callers, and consumers","")); packet_targets={(row["path"],row["symbol"],row["why"]) for row in rows}
        if manifest_targets != packet_targets: errors.append(f"{tid} target path/symbol/why mismatch")
        for row in rows:
            if row["callers"].lower() in {"none","n/a"} or row["consumers"].lower() in {"none","n/a"}:
                if "evidence:" not in row["callers"].lower()+row["consumers"].lower(): errors.append(f"{tid} callers/consumers require names or evidence-backed none")
            if "/" not in row["path"] and "." not in row["path"]: errors.append(f"{tid} target uses only a broad crate label")
            for cell in (row["callers"],row["consumers"]):
                for modified in re.findall(r"modified:([^,;\s]+)",cell):
                    if modified not in task_writes: errors.append(f"{tid} modified caller/consumer absent from write_set: {modified}")
        type_body=task_sections.get("Required types and interfaces","")
        if has_new_or_cross and not re.search(r"```(?:rust|python|typescript|ts)\s+.*?\b(?:fn|struct|enum|class|interface|type|def)\b.*?```",type_body,re.S): errors.append(f"{tid} new/cross-owner symbols require typed signature block")
        stateful=any(task.get("lifecycle",{}).get(d,{}).get("applicable") for d in ("shutdown","cancellation","concurrency","recovery"))
        failure_body=task_sections.get("Failure and edge-case matrix","")
        if stateful and not re.search(r"^\|[^\n]+\|[^\n]+\|[^\n]+\|\s*$",failure_body,re.M): errors.append(f"{tid} stateful task requires exact failure-matrix row")
        for proof in task.get("proofs",[]):
            required={"id","requires_integrated","acceptance","coverage","package","target","features","selector","test_kind","test_path","test_name","expected_selected_count","setup","lane","expected_result","preflight_command","acceptance_command","preflight"}
            if set(proof)!=required or proof.get("acceptance") not in task.get("acceptance",[]): errors.append(f"{tid} invalid proof schema")
            proof_id=str(proof.get("id","")); prerequisites=proof.get("requires_integrated",[])
            if not re.fullmatch(rf"{re.escape(str(tid))}-P[1-9][0-9]*",proof_id): errors.append(f"{tid} invalid proof id")
            if not isinstance(prerequisites,list) or len(prerequisites)!=len(set(prerequisites)) or any(value not in tasks or value==tid for value in prerequisites): errors.append(f"{tid} invalid proof prerequisites")
            if isinstance(prerequisites,list): proof_prerequisites[str(tid)].update(str(value) for value in prerequisites)
            command=str(proof.get("preflight_command","")); acceptance_command=str(proof.get("acceptance_command","")); selector=str(proof.get("selector",""))
            kind=proof.get("test_kind")
            if kind not in {"existing","new","command"} or not safe_path(str(proof.get("test_path",""))) or proof.get("test_name")!=selector or not isinstance(proof.get("expected_selected_count"),int) or proof.get("expected_selected_count",0)<=0: errors.append(f"{tid} invalid planned test identity")
            if not (command.startswith("mise ") or command.startswith("uv ") or command.startswith("pnpm ")): errors.append(f"{tid} command is not exact/auditable")
            if not audited_acceptance(acceptance_command,selector): errors.append(f"{tid} acceptance command must select named test through an audited lane")
            if proof.get("features") and not all(str(feature) in acceptance_command for feature in proof.get("features",[])): errors.append(f"{tid} acceptance command omits declared features")
            if proof.get("setup") != "none" and str(proof.get("setup")) not in acceptance_command: errors.append(f"{tid} acceptance command omits wrapper/setup")
            if kind=="existing" and selector not in command: errors.append(f"{tid} existing-test preflight lacks selector")
            if kind=="new" and "--no-run" not in command: errors.append(f"{tid} new-test preflight must compile with --no-run")
            test_at_source=git(repo,"show",f"{revision}:{proof.get('test_path','')}")
            if kind=="existing" and (test_at_source.returncode or selector.split("::")[-1] not in test_at_source.stdout): errors.append(f"{tid} existing test identity absent at source commit")
            if kind in {"new","command"} and git(repo,"cat-file","-e",f"{revision}:{PurePosixPath(str(proof.get('test_path',''))).parent}").returncode: errors.append(f"{tid} planned test/command parent absent at source commit")
            pre=proof.get("preflight",{}); ep=plan_dir/str(pre.get("path",""))
            if not safe_path(str(pre.get("path",""))) or not ep.is_file() or sha256(ep)!=pre.get("digest"): errors.append(f"{tid} preflight evidence digest invalid"); continue
            try: evidence=json.loads(ep.read_text())
            except Exception: errors.append(f"{tid} preflight evidence invalid JSON"); continue
            common_valid=evidence.get("command")==command and isinstance(evidence.get("exit_code"),int) and isinstance(evidence.get("selected_count"),int) and bool(str(evidence.get("output","")).strip()) and evidence.get("repository")=={"origin":origin,"revision":revision} and evidence.get("package")==proof.get("package") and evidence.get("target")==proof.get("target")
            blocked=evidence.get("classification")=="prerequisite" and evidence.get("requires_integrated")==prerequisites and bool(prerequisites) and evidence.get("exit_code")!=0 and evidence.get("selected_count")==0
            passed=evidence.get("exit_code")==0 and evidence.get("classification") in {None,"pass"}
            selection_valid=(kind=="existing" and evidence.get("selected_count",0)>0) or (kind=="new" and evidence.get("selected_count")==0) or (kind=="command" and evidence.get("selected_count") in {0,1})
            if not common_valid or not ((passed and selection_valid) or blocked): errors.append(f"{tid} preflight evidence violates {kind}-proof contract")
        proof_ids=[p.get("id") for p in task.get("proofs",[])]
        if len(proof_ids)!=len(set(proof_ids)): errors.append(f"{tid} proof ids must be unique")
        proof_acs=[p.get("acceptance") for p in task.get("proofs",[])]
        if set(proof_acs)!=set(task.get("acceptance",[])): errors.append(f"{tid} every acceptance criterion requires proof")
        required_coverage={value for item in task.get("write_set",[]) for value in item.get("coverage",[])}
        actual_coverage={value for proof in task.get("proofs",[]) for value in proof.get("coverage",[])}
        if required_coverage != actual_coverage: errors.append(f"{tid} proof coverage does not match affected surfaces")
        if any(entry.get("proof") not in task.get("acceptance",[]) for entry in task.get("lifecycle",{}).values() if isinstance(entry,dict)): errors.append(f"{tid} lifecycle proof belongs to another task")
    if all_task_acs != plan_acs: errors.append("acceptance criteria are not bidirectionally traceable")
    visiting:set[str]=set(); visited:set[str]=set()
    def visit(n:str)->None:
        if n in visiting: errors.append(f"task DAG cycle at {n}"); return
        if n in visited:return
        visiting.add(n)
        for dep in graph.get(n,[]):
            if dep not in graph: errors.append(f"unknown dependency {dep}")
            else: visit(dep)
        visiting.remove(n); visited.add(n)
    for n in graph: visit(n)
    combined={task:list(set(graph.get(task,[]))|proof_prerequisites.get(task,set())) for task in graph}
    combined_visiting:set[str]=set(); combined_visited:set[str]=set()
    def visit_combined(task_id:str)->None:
        if task_id in combined_visiting: errors.append(f"combined implementation/proof cycle at {task_id}"); return
        if task_id in combined_visited:return
        combined_visiting.add(task_id)
        for prerequisite in combined.get(task_id,[]):
            if prerequisite in combined: visit_combined(prerequisite)
        combined_visiting.remove(task_id); combined_visited.add(task_id)
    for task_id in combined: visit_combined(task_id)
    def ordered(a:str,b:str,seen:set[str]|None=None)->bool:
        seen=set() if seen is None else seen
        if a in seen:return False
        seen.add(a)
        return b in graph.get(a,[]) or any(ordered(d,b,seen) for d in graph.get(a,[]))
    tids=sorted(graph)

    optimization = data.get("optimization", {})
    if optimization.get("objective") != "concurrency_correctness_speed":
        errors.append("optimization objective must be concurrency_correctness_speed")
    if optimization.get("implementor_slots") != 3:
        errors.append("optimization must model exactly three implementor slots")
    candidates = optimization.get("candidate_decompositions", [])
    selected_name = optimization.get("selected_decomposition")
    if not isinstance(candidates, list) or len(candidates) < 2:
        errors.append("optimization requires at least two candidate decompositions")
        candidates = []
    names: set[str] = set()
    selected_metrics: dict[str, float | int] | None = None
    for candidate in candidates:
        if not isinstance(candidate, dict):
            errors.append("invalid candidate decomposition")
            continue
        name = str(candidate.get("name", ""))
        candidate_graph = candidate.get("graph", {})
        if not name or name in names or not isinstance(candidate_graph, dict):
            errors.append("candidate decomposition names and graphs must be unique")
            continue
        names.add(name)
        normalized = {str(task): deps for task, deps in candidate_graph.items()}
        metrics = graph_metrics(normalized, 3)
        if metrics is None:
            errors.append(f"candidate decomposition is not a valid DAG: {name}")
            continue
        for field, actual in metrics.items():
            if candidate.get(field) != actual:
                errors.append(f"candidate decomposition metric mismatch: {name}/{field}")
        if not str(candidate.get("tradeoff", "")).strip():
            errors.append(f"candidate decomposition lacks tradeoff: {name}")
        if name == selected_name:
            selected_metrics = metrics
            if normalized != graph:
                errors.append("selected decomposition graph must equal task dependency graph")
    if selected_name not in names:
        errors.append("selected decomposition is absent")

    direct_edges = {(dep, task) for task, deps in graph.items() for dep in deps}
    serialized: set[tuple[str, str]] = set()
    for edge in optimization.get("serialization_edges", []):
        if not isinstance(edge, dict) or set(edge) != {"before", "after", "consumed_identity", "evidence"}:
            errors.append("invalid serialization edge schema")
            continue
        pair = (str(edge.get("before", "")), str(edge.get("after", "")))
        if pair not in direct_edges or not str(edge.get("consumed_identity", "")).strip() or not str(edge.get("evidence", "")).strip():
            errors.append(f"serialization edge lacks direct dependency identity: {pair[0]}/{pair[1]}")
        serialized.add(pair)
    if serialized != direct_edges:
        errors.append("every direct implementation dependency requires one serialization edge")

    if selected_metrics is not None:
        task_count = len(graph)
        misses = (
            selected_metrics["critical_path_tasks"] / task_count > 0.70
            or selected_metrics["max_runnable_width"] < 2
            or selected_metrics["average_occupied_slots"] < 1.5
            or selected_metrics["parallel_task_fraction"] < 0.35
        )
        waiver = optimization.get("concurrency_waiver")
        if misses:
            evidence = waiver.get("evidence", []) if isinstance(waiver, dict) else []
            accepted = waiver.get("accepted_by_user") if isinstance(waiver, dict) else False
            joined = " ".join(str(value) for value in evidence).lower()
            if not accepted or not evidence or not all(word in joined for word in ("foundation", "module", "join")):
                errors.append("selected graph misses concurrency thresholds without an explicit source-backed user waiver")
        elif waiver not in (None, {}):
            errors.append("concurrency waiver must be absent when selected graph meets thresholds")

    for i,a in enumerate(tids):
        for b in tids[i+1:]:
            if any(overlaps(left,right) for left in writes[a] for right in writes[b]) and not ordered(a,b) and not ordered(b,a): errors.append(f"parallel write overlap {a}/{b}")
    for lock in data.get("locks",[]):
        owners=lock.get("owners",[])
        if lock.get("category") not in LOCK_CATEGORIES or not str(lock.get("key","")).strip() or not owners or any(o not in tasks for o in owners): errors.append("invalid lock")
        for i,a in enumerate(owners):
            for b in owners[i+1:]:
                if not ordered(a,b) and not ordered(b,a): errors.append(f"lock owners not dependency ordered: {a}/{b}")
    lock_map={(lock.get("category"),lock.get("key")):set(lock.get("owners",[])) for lock in data.get("locks",[]) if isinstance(lock,dict)}
    for tid, paths in writes.items():
        for path in paths:
            category=derived_lock(path)
            if category and tid not in lock_map.get((category,path),set()): errors.append(f"{tid} omitted derived {category} lock for {path}")
    for rid, owners in data.get("requirements",{}).items():
        actual={tid for tid,task in tasks.items() if rid in task.get("requirements",[])}
        if not isinstance(owners,list) or set(owners)!=actual or not actual: errors.append(f"requirement owner list mismatch: {rid}")
    rehearsal=plan_dir/"rehearsal.json"
    ready=plan_status=="Approved" or any(metadata((plan_dir/t["packet"]).read_text(),"Status")=="Ready" for t in tasks.values() if (plan_dir/t["packet"]).is_file())
    if ready:
        if not rehearsal.is_file(): errors.append("Approved/Ready artifacts require rehearsal.json PASS")
        else:
            try: record=json.loads(rehearsal.read_text())
            except Exception: record={}
            if record.get("verdict")!="PASS" or record.get("manifest_digest")!=sha256(mp) or record.get("artifact_digests")!=artifacts or record.get("repository")!={"origin":origin,"revision":revision}: errors.append("rehearsal PASS binding mismatch")
    return errors

def main()->int:
    """Validate a plan directory against an explicit repository root."""
    parser=argparse.ArgumentParser(); parser.add_argument("plan_directory",type=Path); parser.add_argument("--repository-root",type=Path,required=True); args=parser.parse_args()
    errors=validate(args.plan_directory,args.repository_root)
    if errors: print("\n".join(errors),file=sys.stderr); return 1
    print(f"valid v3 plan artifacts: {args.plan_directory}"); return 0
if __name__=="__main__": raise SystemExit(main())
