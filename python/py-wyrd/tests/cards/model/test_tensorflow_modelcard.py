from __future__ import annotations

from pathlib import Path

import pytest
from _helpers import assert_model_card_json, model_metadata
from wyrd.model import ModelCard, SampleInput, TensorflowInterface

pytestmark = pytest.mark.tensorflow


def _tensorflow_model():
    import tensorflow as tf

    return tf.keras.Sequential(
        [
            tf.keras.Input(shape=(2,)),
            tf.keras.layers.Dense(1),
        ]
    )


def test_tensorflow_interface_round_trips_with_keras_artifact(tmp_path: Path) -> None:
    card = ModelCard(
        TensorflowInterface(model=_tensorflow_model(), save_format="keras"),
        metadata=model_metadata("regression"),
    )
    path = tmp_path / "tensorflow"

    card.save(path)
    restored = ModelCard.model_validate_json((path / "card.json").read_text())
    restored.load(path)

    assert restored.interface.kind == "Tensorflow"
    assert restored.interface.has_model is True
    assert restored.interface.save_format == "keras"
    assert (path / "model.keras").exists()
    assert_model_card_json(path, "Tensorflow")


def test_tensorflow_artifact_path_modelcard_save_and_load(tmp_path: Path) -> None:
    source = tmp_path / "source-tensorflow"
    ModelCard(
        TensorflowInterface(model=_tensorflow_model(), save_format="keras"),
        metadata=model_metadata("regression"),
    ).save(source)

    card = ModelCard(source / "model.keras", metadata=model_metadata("regression"))
    path = tmp_path / "tensorflow-from-path"
    card.save(path)
    restored = ModelCard.model_validate_json((path / "card.json").read_text())
    restored.load(path)

    assert restored.interface.kind == "Tensorflow"
    assert restored.interface.has_model is True
    assert (path / "model.keras").exists()
    assert_model_card_json(path, "Tensorflow")


def test_tensorflow_sample_input_saves_as_numpy_file(tmp_path: Path) -> None:
    import tensorflow as tf

    sample = SampleInput.from_python_object(tf.constant([[1.0, 2.0]]))
    path = tmp_path / "tf-sample"

    assert sample.kind_token == "tf"
    sample.save(path)

    assert (path / "sample_input.npy").exists()
    loaded = SampleInput(kind="tensorflow")
    loaded.load(path)
    assert loaded.has_value is True
