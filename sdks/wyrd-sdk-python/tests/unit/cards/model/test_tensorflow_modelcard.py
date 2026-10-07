"""A Keras model saves and loads through the Tensorflow interface."""

from pathlib import Path
from typing import Any

import pytest
from wyrd.model import ModelCard, ModelCardMetadata, SampleInput, TensorflowInterface

pytestmark = pytest.mark.tensorflow


@pytest.fixture
def keras_model() -> Any:
    """A one-layer Keras regressor over two features."""
    import keras

    return keras.Sequential([keras.Input(shape=(2,)), keras.layers.Dense(1)])


@pytest.mark.parametrize("save_format", ["keras", "savedmodel"])
def test_keras_model_round_trips_in_its_save_format(
    tmp_path: Path, keras_model: Any, regressor_metadata: ModelCardMetadata, save_format: str
) -> None:
    ModelCard(
        TensorflowInterface(model=keras_model, save_format=save_format), metadata=regressor_metadata
    ).save(tmp_path)

    restored = ModelCard.from_path(tmp_path)

    assert isinstance(restored.interface, TensorflowInterface)
    assert (restored.interface.has_model, restored.interface.save_format) == (True, save_format)


def test_loaded_keras_model_saves_as_a_new_card(
    tmp_path: Path, keras_model: Any, regressor_metadata: ModelCardMetadata
) -> None:
    ModelCard(
        TensorflowInterface(model=keras_model, save_format="keras"), metadata=regressor_metadata
    ).save(tmp_path / "source")
    interface = TensorflowInterface(save_format="keras")
    interface.load(tmp_path / "source")
    ModelCard(interface, metadata=regressor_metadata).save(tmp_path / "copy")

    restored = ModelCard.from_path(tmp_path / "copy")

    assert isinstance(restored.interface, TensorflowInterface)
    assert restored.interface.has_model


def test_tensorflow_sample_input_survives_save_and_load(tmp_path: Path) -> None:
    import tensorflow as tf

    SampleInput.from_python_object(tf.constant([[1.0, 2.0]])).save(tmp_path)

    loaded = SampleInput(kind="tensorflow")
    loaded.load(tmp_path)

    assert loaded.has_value is True
