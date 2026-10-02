"""End-to-end Card registration, lookup, artifact loading, and deletion journeys."""

from __future__ import annotations

import json
import os
import threading
from hashlib import sha256
from pathlib import Path
from uuid import uuid4

import pandas as pd
import pytest
from sklearn.linear_model import LogisticRegression
from wyrd import WyrdError
from wyrd.cards import (
    Cards,
    DataLoadArgs,
    DataSaveArgs,
    ModelLoadArgs,
    ModelSaveArgs,
    VersionBump,
)
from wyrd.data import DataCard, DataInterface, DataStats, FieldSpec, PandasInterface
from wyrd.model import (
    ModelCard,
    ModelCardMetadata,
    ModelInterface,
    ModelSignature,
    SklearnInterface,
)
from wyrd.prompt import Prompt, PromptCard


class JsonDataInterface(DataInterface):
    def __init__(self, value: dict[str, int] | None = None) -> None:
        super().__init__()
        self.value = value
        self.saved_kwargs: dict[str, str | bool] | None = None
        self.loaded_kwargs: dict[str, str | bool] | None = None
        self.loaded_path: Path | None = None

    def save(self, path: Path, save_kwargs: dict[str, str | bool] | None = None) -> DataStats:
        self.saved_kwargs = save_kwargs
        payload = json.dumps(self.value or {}, sort_keys=True).encode("utf-8")
        output = path / "data" / "custom" / "data.json"
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_bytes(payload)
        return DataStats(byte_count=len(payload), sha256=sha256(payload).hexdigest())

    def load(self, path: Path, load_kwargs: dict[str, str | bool] | None = None) -> None:
        self.loaded_path = path
        self.loaded_kwargs = load_kwargs
        self.value = json.loads((path / "data" / "custom" / "data.json").read_text())


class SymlinkDataInterface(JsonDataInterface):
    def save(self, path: Path, save_kwargs: dict[str, str | bool] | None = None) -> DataStats:
        stats = super().save(path, save_kwargs)
        source = path / "data" / "custom" / "source.bin"
        source.write_bytes(b"source")
        (path / "data" / "custom" / "linked.bin").symlink_to(source)
        return stats


class LargeDataInterface(DataInterface):
    """Write a bounded artifact and mark the start of native preparation."""

    def __init__(self, counter: list[int]) -> None:
        super().__init__()
        self.counter = counter

    def save(self, path: Path, save_kwargs=None) -> DataStats:
        """Write deterministic bytes, then reset progress before native hashing."""
        del save_kwargs
        payload = b"x" * (512 * 1024)
        output = path / "data" / "large.bin"
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_bytes(payload)
        self.counter[0] = 0
        return DataStats(byte_count=len(payload), sha256=sha256(payload).hexdigest())

    def load(self, path: Path, load_kwargs=None) -> None:
        """Provide the abstract loader without use in this registration test."""
        del path, load_kwargs


class TextModelInterface(ModelInterface):
    def __init__(self, value: str = "") -> None:
        super().__init__()
        self.value = value
        self.saved_kwargs: dict[str, str | bool] | None = None
        self.loaded_kwargs: dict[str, str | bool] | None = None

    def save(self, path: Path, save_kwargs: dict[str, str | bool] | None = None) -> None:
        self.saved_kwargs = save_kwargs
        path.mkdir(parents=True, exist_ok=True)
        (path / "model.txt").write_text(self.value, encoding="utf-8")

    def load(self, path: Path, load_kwargs: dict[str, str | bool] | None = None) -> None:
        self.loaded_kwargs = load_kwargs
        self.value = (path / "model.txt").read_text(encoding="utf-8")

    @property
    def model(self) -> str:
        return self.value


def _name(prefix: str) -> str:
    return f"{prefix}-{uuid4().hex[:12]}"


def _model_metadata() -> ModelCardMetadata:
    signature = ModelSignature(
        [FieldSpec("feature", "float64")],
        [FieldSpec("prediction", "float64")],
    )
    return ModelCardMetadata(task_type="other", signature=signature)


def _cards(wyrd_server) -> Cards:
    api_key = wyrd_server.bootstrap_service(["writer"], name=_name("cards-client"))
    os.environ["WYRD_SERVER_URL"] = wyrd_server.base_url
    os.environ["WYRD_API_KEY"] = api_key
    return Cards()


@pytest.mark.integration
def test_prompt_cards_register_get_list_resolve_latest_and_delete(wyrd_server) -> None:
    cards = _cards(wyrd_server)
    name = _name("prompt")
    card = PromptCard(
        Prompt.openai_chat("gpt-4o", messages="Hello {{name}}"),
        space="python-e2e",
        name=name,
        version="0.1.0",
    )

    receipt = cards.register(card)

    assert card.uid == receipt.root.uid
    assert receipt.root.name == name
    envelope = cards.prompt.get(uid=card.uid)
    assert isinstance(envelope, PromptCard)
    assert envelope.uid == card.uid
    assert isinstance(envelope.prompt, Prompt)
    assert envelope.prompt.model == "gpt-4o"
    latest = cards.prompt.resolve_latest(space="python-e2e", name=name)
    assert latest.uid == card.uid
    exact = cards.prompt.get(space="python-e2e", name=name, version=latest.version)
    assert exact.uid == card.uid
    page = cards.prompt.list(space="python-e2e", name=name)
    assert any(item.uid == card.uid and item.status == "active" for item in page.items)

    cards.prompt.delete(uid=card.uid)
    with pytest.raises(WyrdError):
        cards.prompt.get(uid=card.uid)


@pytest.mark.integration
def test_data_card_custom_interface_get_requires_interface_and_loads_artifacts(wyrd_server) -> None:
    cards = _cards(wyrd_server)
    name = _name("data")
    interface = JsonDataInterface({"rows": 2})
    card = DataCard(interface, space="python-e2e", name=name, version="0.1.0")

    cards.data.register(
        card,
        save_args=DataSaveArgs({"compression": "none"}, copy_bytes=True),
    )

    assert card.uid
    assert interface.saved_kwargs == {"compression": "none", "copy_bytes": True}
    with pytest.raises(WyrdError):
        cards.data.get(uid=card.uid)

    unloaded = cards.data.get(uid=card.uid, interface=JsonDataInterface)
    with pytest.raises(WyrdError):
        unloaded.load(load_kwargs=DataLoadArgs({"strict": True}))

    loaded = cards.data.get(
        uid=card.uid,
        interface=JsonDataInterface,
        eager_load=True,
        load_kwargs=DataLoadArgs({"strict": True}),
    )
    assert loaded.uid == card.uid
    assert loaded.interface.value == {"rows": 2}
    assert loaded.interface.loaded_kwargs == {"strict": True}

    cards.data.delete(uid=card.uid)


@pytest.mark.integration
def test_data_card_pandas_interface_eager_loads_after_real_registry_round_trip(wyrd_server) -> None:
    """A Pandas DataCard retains its tabular values through eager artifact hydration."""
    cards = _cards(wyrd_server)
    data = pd.DataFrame({"customer_id": [101, 202], "score": [0.25, 0.75]})
    card = DataCard(
        PandasInterface(data=data),
        space="python-e2e",
        name=_name("pandas-data"),
        version="0.1.0",
    )

    cards.data.register(card)
    loaded = cards.data.get(uid=card.uid, interface=PandasInterface, eager_load=True)

    assert loaded.uid == card.uid
    pd.testing.assert_frame_equal(loaded.data, data)

    cards.data.delete(uid=card.uid)
    with pytest.raises(WyrdError):
        cards.data.get(uid=card.uid, interface=PandasInterface)


@pytest.mark.integration
def test_eager_data_load_uses_constructed_server_and_retains_workspace(wyrd_server) -> None:
    """A Cards handle stays bound to server A when ambient config changes to B."""
    api_key = wyrd_server.bootstrap_service(["writer"], name=_name("eager-client"))
    cards = Cards(server_url=wyrd_server.base_url, credential=api_key)
    interface = JsonDataInterface({"rows": 3})
    card = DataCard(
        interface,
        space="python-e2e",
        name=_name("eager-data"),
        version="0.1.0",
    )
    cards.data.register(card)

    os.environ["WYRD_SERVER_URL"] = "http://127.0.0.1:1"
    os.environ["WYRD_API_KEY"] = "server-b-must-not-be-used"
    loaded = cards.data.get(uid=card.uid, interface=JsonDataInterface, eager_load=True)

    assert loaded.interface.value == {"rows": 3}
    assert loaded.interface.loaded_path is not None
    assert loaded.interface.loaded_path.exists()
    cards.data.delete(uid=card.uid)


@pytest.mark.integration
def test_model_card_custom_interface_get_requires_interface_and_loads_artifacts(
    wyrd_server,
) -> None:
    cards = _cards(wyrd_server)
    name = _name("model")
    interface = TextModelInterface("model-bytes")
    card = ModelCard(
        interface,
        space="python-e2e",
        name=name,
        version="0.1.0",
        metadata=_model_metadata(),
    )

    cards.model.register(card, save_args=ModelSaveArgs({"format": "text"}))

    assert card.uid
    assert interface.saved_kwargs == {"format": "text"}
    with pytest.raises(WyrdError):
        cards.model.get(uid=card.uid)

    unloaded = cards.model.get(uid=card.uid, interface=TextModelInterface)
    with pytest.raises(WyrdError):
        unloaded.load(load_kwargs=ModelLoadArgs({"strict": True}))

    loaded = cards.model.get(
        uid=card.uid,
        interface=TextModelInterface,
        eager_load=True,
        load_kwargs=ModelLoadArgs({"strict": True}),
    )
    assert loaded.interface.value == "model-bytes"
    assert loaded.interface.loaded_kwargs == {"strict": True}
    assert loaded.model == "model-bytes"
    assert loaded.preprocessor is None
    assert loaded.processor is None

    cards.model.delete(uid=card.uid)


@pytest.mark.integration
def test_model_card_sklearn_interface_eager_loads_after_real_registry_round_trip(
    wyrd_server,
) -> None:
    """A scikit-learn ModelCard remains executable after eager artifact hydration."""
    cards = _cards(wyrd_server)
    features = [[0.0], [1.0], [2.0], [3.0]]
    labels = [0, 0, 1, 1]
    card = ModelCard(
        SklearnInterface(model=LogisticRegression(random_state=0).fit(features, labels)),
        space="python-e2e",
        name=_name("sklearn-model"),
        version="0.1.0",
        metadata=_model_metadata(),
    )

    cards.model.register(card)
    loaded = cards.model.get(uid=card.uid, eager_load=True)

    assert loaded.uid == card.uid
    assert loaded.model.predict([[0.0], [3.0]]).tolist() == [0, 1]

    cards.model.delete(uid=card.uid)
    with pytest.raises(WyrdError):
        cards.model.get(uid=card.uid)


@pytest.mark.integration
def test_typed_registry_rejects_a_card_of_the_wrong_kind(wyrd_server) -> None:
    cards = _cards(wyrd_server)
    card = PromptCard(Prompt.openai_chat("gpt-4o", messages="Hello"), name=_name("wrong"))

    with pytest.raises(WyrdError):
        cards.data.register(card)


@pytest.mark.integration
def test_card_registration_replay_is_idempotent(wyrd_server) -> None:
    cards = _cards(wyrd_server)
    card = PromptCard(
        Prompt.openai_chat("gpt-4o", messages="Replay me"),
        space="python-e2e",
        name=_name("replay"),
        version="0.1.0",
    )

    first = cards.prompt.register(card)
    second = cards.prompt.register(card)

    assert first.root.uid == second.root.uid == card.uid
    assert second.outcomes[0].outcome == "idempotent_noop"
    cards.prompt.delete(uid=card.uid)


@pytest.mark.integration
def test_registration_version_modes_match_server_contract(wyrd_server) -> None:
    """Omitted/Patch/Minor bumps and exact pins resolve without mixed intent."""
    cards = _cards(wyrd_server)
    name = _name("versions")

    omitted = PromptCard(
        Prompt.openai_chat("gpt-4o", messages="version"),
        space="python-e2e",
        name=name,
        version="1",
    )
    assert cards.prompt.register(omitted).root.version == "1.0.0"
    assert omitted.version == "1.0.0"

    patch = PromptCard(
        Prompt.openai_chat("gpt-4o", messages="version changed"),
        space="python-e2e",
        name=name,
        version="1",
    )
    assert cards.prompt.register(patch, version_bump=VersionBump.Patch).root.version == "1.0.1"
    assert patch.version == "1.0.1"

    minor = PromptCard(
        Prompt.openai_chat("gpt-4o", messages="version changed again"),
        space="python-e2e",
        name=name,
        version="1",
    )
    assert cards.prompt.register(minor, version_bump=VersionBump.Minor).root.version == "1.1.0"
    assert minor.version == "1.1.0"

    exact = PromptCard(
        Prompt.openai_chat("gpt-4o", messages="exact"),
        space="python-e2e",
        name=name,
        version="2.3.4",
    )
    assert cards.prompt.register(exact).root.version == "2.3.4"
    assert exact.version == "2.3.4"


@pytest.mark.integration
def test_registration_releases_gil_and_failure_preserves_holder_identity(wyrd_server) -> None:
    """Native hashing/network permit Python progress and stamp only on success."""
    cards = _cards(wyrd_server)
    counter = [0]
    stop = threading.Event()

    def compete() -> None:
        """Increment while registration releases the interpreter lock."""
        while not stop.is_set():
            counter[0] += 1

    thread = threading.Thread(target=compete)
    thread.start()
    card = DataCard(
        LargeDataInterface(counter),
        space="python-e2e",
        name=_name("gil"),
        version="0.1.0",
    )
    try:
        cards.data.register(card)
    finally:
        stop.set()
        thread.join()
    assert counter[0] > 0

    denied_key = wyrd_server.bootstrap_service([], name=_name("denied-register"))
    denied = Cards(server_url=wyrd_server.base_url, credential=denied_key)
    unchanged = PromptCard(
        Prompt.openai_chat("gpt-4o", messages="identity"),
        space="python-e2e",
        name=_name("unchanged"),
        version="3.2.1",
    )
    original = (unchanged.uid, unchanged.version)
    with pytest.raises(WyrdError):
        denied.prompt.register(unchanged)
    assert (unchanged.uid, unchanged.version) == original


@pytest.mark.integration
def test_card_registration_rejects_invalid_artifact_layout(wyrd_server) -> None:
    cards = _cards(wyrd_server)
    card = DataCard(
        SymlinkDataInterface({"rows": 1}),
        space="python-e2e",
        name=_name("symlink"),
        version="0.1.0",
    )

    with pytest.raises(WyrdError):
        cards.data.register(card)


@pytest.mark.integration
def test_card_registration_rejects_underprivileged_writer(wyrd_server) -> None:
    api_key = wyrd_server.bootstrap_service([], name=_name("read-only"))
    cards = Cards(server_url=wyrd_server.base_url, credential=api_key)
    card = PromptCard(
        Prompt.openai_chat("gpt-4o", messages="No write permission"),
        space="python-e2e",
        name=_name("forbidden"),
        version="0.1.0",
    )

    with pytest.raises(WyrdError):
        cards.prompt.register(card)


@pytest.mark.integration
def test_card_registry_enforces_cross_tenant_isolation(wyrd_server) -> None:
    cards = _cards(wyrd_server)
    card = PromptCard(
        Prompt.openai_chat("gpt-4o", messages="Tenant A"),
        space="python-e2e",
        name=_name("tenant"),
        version="0.1.0",
    )
    cards.prompt.register(card)

    tenant_b = wyrd_server.seed_tenant(_name("tenant-b"))
    tenant_b_key = wyrd_server.bootstrap_service_in_tenant(
        tenant_b,
        ["writer"],
        name=_name("tenant-b-client"),
    )
    cards_b = Cards(server_url=wyrd_server.base_url, credential=tenant_b_key)

    with pytest.raises(WyrdError):
        cards_b.prompt.get(uid=card.uid)
    with pytest.raises(WyrdError):
        cards_b.prompt.delete(uid=card.uid)

    cards.prompt.delete(uid=card.uid)


_REPO = Path(__file__).parents[5]
_FIXTURES = _REPO / "tests" / "fixtures" / "workflow-loading"


def _uids(receipt) -> dict[str, str]:
    return {outcome.card_ref.name: outcome.card_ref.uid for outcome in receipt.outcomes}


# The fixture Prompts send their Native Chat request to the built-in `mock`
# provider, which answers with the rendered user message. Each output therefore
# shows which Prompt body ran and what was bound into it.
_LOCAL_REVIEW = (
    "final review of diff | local security review of diff | local correctness review of diff"
)
_REGISTERED_REVIEW = (
    "final review of diff"
    " | registered security review of diff"
    " | registered correctness review of diff"
)


@pytest.mark.integration
def test_workflow_loading_journey(wyrd_server, tmp_path: Path, monkeypatch) -> None:
    """Load and run Workflow files and registered Workflows; see tests/fixtures/workflow-loading."""
    from wyrd.agent import Workflow

    writer_key = wyrd_server.bootstrap_service(["writer"], name=_name("workflow-writer"))
    reader_key = wyrd_server.bootstrap_service(["reader"], name=_name("workflow-reader"))
    no_roles_key = wyrd_server.bootstrap_service([], name=_name("workflow-no-roles"))
    writer = Cards(server_url=wyrd_server.base_url, credential=writer_key)
    reader = Cards(server_url=wyrd_server.base_url, credential=reader_key)
    no_roles = Cards(server_url=wyrd_server.base_url, credential=no_roles_key)

    # Workflow.from_path reads credentials from the environment, so start with none.
    monkeypatch.setenv("WYRD_SERVER_URL", wyrd_server.base_url)
    monkeypatch.setenv("WYRD_CONFIG_HOME", str(tmp_path / "config"))
    monkeypatch.delenv("WYRD_API_KEY", raising=False)
    monkeypatch.delenv("WYRD_ACCESS_TOKEN", raising=False)

    # 1. Wholly local Workflow files load and run without credentials.
    local = Workflow.from_path(_FIXTURES / "shadowed" / "local-workflow.yaml")
    run = local.run({"code": "diff"})
    assert run.status == "succeeded"
    assert run.outputs == {"security": "local security review of diff", "review": _LOCAL_REVIEW}

    # The code-review example calls models through the Wyrd gateway, which a
    # plain run does not have, so its run is refused before any step starts.
    example = Workflow.from_path(_REPO / "examples" / "workflows" / "code-review" / "workflow.yaml")
    assert list(example.steps) == ["security", "correctness", "final_review"]
    with pytest.raises(WyrdError) as unavailable:
        example.run({"code": "diff"})
    assert unavailable.value.code == "WYRD_WORKFLOW_503_BINDING_UNAVAILABLE"

    # 2. The team registers its reviewer Agents.
    team = _uids(writer.register_from_path(_FIXTURES / "team" / "security.yaml"))
    team.update(_uids(writer.register_from_path(_FIXTURES / "team" / "correctness.yaml")))

    # 3. A file referencing registered Agents needs a credential that can read them.
    mixed = _FIXTURES / "mixed" / "workflow.yaml"
    with pytest.raises(WyrdError) as missing:
        Workflow.from_path(mixed)
    assert missing.value.code == "WYRD_CLIENT_401_NO_CREDENTIALS"

    monkeypatch.setenv("WYRD_API_KEY", no_roles_key)
    with pytest.raises(WyrdError) as denied:
        Workflow.from_path(mixed)
    assert denied.value.code == "WYRD_PERMISSION_403_DENIED_RBAC"

    monkeypatch.setenv("WYRD_API_KEY", reader_key)
    run = Workflow.from_path(str(mixed)).run({"code": "diff"})
    assert run.status == "succeeded"
    assert run.outputs == {"review": _REGISTERED_REVIEW}

    # 4. A local sibling and the registered Agent with the same identity each
    #    run their own Prompt.
    run = Workflow.from_path(_FIXTURES / "shadowed" / "workflow.yaml").run({"code": "diff"})
    assert run.status == "succeeded"
    assert run.outputs == {
        "security": "local security review of diff",
        "registered_security": "registered security review of diff",
        "review": _LOCAL_REVIEW,
    }

    # 5. A reference to a deleted Card is refused.
    retired = _uids(writer.register_from_path(_FIXTURES / "retired" / "retired-prompt.yaml"))
    writer.prompt.delete(uid=retired["retired-prompt"])
    with pytest.raises(WyrdError) as inactive:
        Workflow.from_path(_FIXTURES / "retired" / "workflow.yaml")
    assert inactive.value.code == "WYRD_REGISTRY_404_CARD_NOT_FOUND"

    # 6. Apply the mixed Workflow, register a newer security Agent, then load
    #    the applied Workflow by identity and by UID: both stay pinned to 1.0.0
    #    and never run the newer Prompt ("v2 security review of diff").
    workflow_uid = _uids(writer.register_from_path(mixed))["code-review"]
    newer = _uids(writer.register_from_path(_FIXTURES / "team-v2" / "security.yaml"))
    by_identity = reader.workflow.load(
        space="workflow-loading", name="code-review", version="1.0.0"
    )
    by_uid = reader.workflow.load(uid=workflow_uid)
    for workflow in [by_identity, by_uid]:
        assert workflow.version == "1.0.0"
        stored = workflow.to_yaml()
        assert team["security-reviewer"] in stored
        assert team["correctness-reviewer"] in stored
        assert newer["security-reviewer"] not in stored
        run = workflow.run({"code": "diff"})
        assert run.status == "succeeded"
        assert run.outputs == {"review": _REGISTERED_REVIEW}
        assert run.steps["final_review"]["text"] == _REGISTERED_REVIEW

    # 7. Incomplete, mixed, wrong-kind, and unauthorized selectors are refused.
    with pytest.raises(WyrdError) as versionless:
        reader.workflow.load(space="workflow-loading", name="code-review")
    assert versionless.value.code == "WYRD_WORKFLOW_400_INVALID_CARD_REF"
    with pytest.raises(WyrdError) as mixed_selector:
        reader.workflow.load(uid=workflow_uid, space="workflow-loading")
    assert mixed_selector.value.code == "WYRD_WORKFLOW_400_INVALID_CARD_REF"
    # An Agent's UID names no Workflow.
    with pytest.raises(WyrdError) as wrong_kind:
        reader.workflow.load(uid=team["security-reviewer"])
    assert wrong_kind.value.code == "WYRD_REGISTRY_404_CARD_NOT_FOUND"
    with pytest.raises(WyrdError) as unauthorized:
        no_roles.workflow.load(uid=workflow_uid)
    assert unauthorized.value.code == "WYRD_PERMISSION_403_DENIED_RBAC"
