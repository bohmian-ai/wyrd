"""Register a table from nested Pydantic models, write rows, and query inside them.

Nested models register as struct columns, queried by path:
``prediction['feature_importance']['importance']``. A ``dict[str, Any]`` field
registers as a Variant column for open data, queried with ``->`` and ``->>``.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

import pytest
from pydantic import BaseModel
from wyrd.bifrost import Bifrost, TableConfig

if TYPE_CHECKING:
    from wyrd.testing import WyrdTestServer


class FeatureImportance(BaseModel):
    name: str
    importance: float


class Prediction(BaseModel):
    result: int
    probabilities: list[float]
    feature_importance: FeatureImportance


class ApiRequest(BaseModel):
    request_id: str
    outcome: int
    prediction: Prediction
    attributes: dict[str, Any]


REQUESTS = [
    ApiRequest(
        request_id="req-1",
        outcome=1,
        prediction=Prediction(
            result=1,
            probabilities=[0.1, 0.9],
            feature_importance=FeatureImportance(name="tenure", importance=0.72),
        ),
        attributes={"client": {"region": "us-east"}, "retries": 0},
    ),
    ApiRequest(
        request_id="req-2",
        outcome=0,
        prediction=Prediction(
            result=0,
            probabilities=[0.8, 0.2],
            feature_importance=FeatureImportance(name="spend", importance=0.31),
        ),
        attributes={"client": {"region": "eu-west"}, "retries": 2, "coupon": None},
    ),
]


@pytest.fixture(scope="module")
def api_requests(wyrd_server: WyrdTestServer) -> Bifrost:
    """Register the ``vala.datasets.api_requests`` table and write ``REQUESTS``."""

    api_requests = Bifrost(TableConfig(ApiRequest, "vala.datasets.api_requests"))
    api_requests.register()
    for request in REQUESTS:
        api_requests.insert(request)
    api_requests.flush()
    wyrd_server.flush_bifrost()
    return api_requests


class Importance(BaseModel):
    request_id: str
    importance: float


class Region(BaseModel):
    request_id: str
    region: str


@pytest.mark.integration
def test_api_requests_read_back_as_the_same_models(api_requests: Bifrost) -> None:
    rows = api_requests.sql(
        "SELECT request_id, outcome, prediction, attributes "
        "FROM vala.datasets.api_requests ORDER BY request_id",
        ApiRequest,
    )

    assert rows == REQUESTS


@pytest.mark.integration
def test_a_nested_model_field_is_selected_by_its_path(api_requests: Bifrost) -> None:
    rows = api_requests.sql(
        "SELECT request_id, prediction['feature_importance']['importance'] AS importance "
        "FROM vala.datasets.api_requests ORDER BY request_id",
        Importance,
    )

    assert rows == [
        Importance(request_id="req-1", importance=0.72),
        Importance(request_id="req-2", importance=0.31),
    ]


@pytest.mark.integration
def test_a_nested_model_field_filters_rows(api_requests: Bifrost) -> None:
    rows = api_requests.sql(
        "SELECT request_id, prediction['feature_importance']['importance'] AS importance "
        "FROM vala.datasets.api_requests "
        "WHERE prediction['feature_importance']['importance'] > 0.5",
        Importance,
    )

    assert rows == [Importance(request_id="req-1", importance=0.72)]


@pytest.mark.integration
def test_an_open_dict_field_is_queried_by_key(api_requests: Bifrost) -> None:
    rows = api_requests.sql(
        "SELECT request_id, attributes -> 'client' ->> 'region' AS region "
        "FROM vala.datasets.api_requests ORDER BY request_id",
        Region,
    )

    assert rows == [
        Region(request_id="req-1", region="us-east"),
        Region(request_id="req-2", region="eu-west"),
    ]


@pytest.mark.integration
def test_registering_the_same_model_again_finds_the_existing_table(
    api_requests: Bifrost,
) -> None:
    assert api_requests.register() == "already_exists"
