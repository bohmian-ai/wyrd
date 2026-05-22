use pythonize::{depythonize, pythonize};

/// Generate a UUIDv7 string.
#[must_use]
pub fn uuid7() -> String {
    uuid::Uuid::new_v7(uuid::Timestamp::now(uuid::NoContext)).to_string()
}

/// Parse and normalize a semantic version.
///
/// # Errors
/// Returns an error when the input is not semver.
pub fn normalize_version(value: &str) -> Result<String, semver::Error> {
    semver::Version::parse(value).map(|version| version.to_string())
}

/// Return a score clamped into the `0.0..=1.0` range.
#[must_use]
pub fn clamp_score(value: f64) -> f64 {
    value.clamp(0.0, 1.0)
}

/// Convert a Python-style identifier into a simple kebab-case token.
#[must_use]
pub fn depythonize(value: &str) -> String {
    value.trim().replace('_', "-").to_ascii_lowercase()
}

pub struct PyHelperFuncs {}

impl PyHelperFuncs {
    /// Convert any type implementing `IntoPyObject` to a Python object
    /// # Arguments
    /// * `py` - A Python interpreter instance
    /// * `object` - A reference to an object implementing `IntoPyObject`
    /// # Returns
    /// * `Result<Bound<'py, PyAny>, UtilError>` - A result containing the Python object or an error
    pub fn to_bound_py_object<'py, T>(
        py: Python<'py>,
        object: &T,
    ) -> Result<Bound<'py, PyAny>, UtilError>
    where
        T: IntoPyObject<'py> + Clone,
    {
        Ok(object.clone().into_bound_py_any(py)?)
    }
    pub fn __str__<T: Serialize>(object: T) -> String {
        match ColoredFormatter::with_styler(
            PrettyFormatter::default(),
            Styler {
                key: Color::Rgb(75, 57, 120).foreground(),
                string_value: Color::Rgb(4, 205, 155).foreground(),
                float_value: Color::Rgb(4, 205, 155).foreground(),
                integer_value: Color::Rgb(4, 205, 155).foreground(),
                bool_value: Color::Rgb(4, 205, 155).foreground(),
                nil_value: Color::Rgb(4, 205, 155).foreground(),
                ..Default::default()
            },
        )
        .to_colored_json(&object, ColorMode::On)
        {
            Ok(json) => json,
            Err(e) => format!("Failed to serialize to json: {e}"),
        }
        // serialize the struct to a string
    }

    pub fn __json__<T: Serialize>(object: T) -> String {
        match serde_json::to_string_pretty(&object) {
            Ok(json) => json,
            Err(e) => format!("Failed to serialize to json: {e}"),
        }
    }

    /// Save a struct to a JSON file
    ///
    /// # Arguments
    ///
    /// * `model` - A reference to a struct that implements the `Serialize` trait
    /// * `path` - A reference to a `Path` object that holds the path to the file
    ///
    /// # Returns
    ///
    /// A `Result` containing `()` or a `UtilError`
    ///
    /// # Errors
    ///
    /// This function will return an error if:
    /// - The struct cannot be serialized to a string
    pub fn save_to_json<T>(model: T, path: &Path) -> Result<(), UtilError>
    where
        T: Serialize,
    {
        // serialize the struct to a string
        let json =
            serde_json::to_string_pretty(&model).map_err(|_| UtilError::SerializationError)?;

        // ensure .json extension
        let path = path.with_extension("json");

        if !path.exists() {
            // ensure path exists, create if not
            let parent_path = path.parent().ok_or(UtilError::GetParentPathError)?;
            if !parent_path.as_os_str().is_empty() {
                std::fs::create_dir_all(parent_path)
                    .map_err(|_| UtilError::CreateDirectoryError)?;
            }
        }

        std::fs::write(path, json).map_err(|_| UtilError::WriteError)?;

        Ok(())
    }
}

pub fn vec_to_py_object<'py>(
    py: Python<'py>,
    vec: &Vec<Value>,
) -> Result<Bound<'py, PyList>, UtilError> {
    let py_list = PyList::empty(py);
    for item in vec {
        let py_item = pythonize(py, item)?;
        py_list.append(py_item)?;
    }
    Ok(py_list)
}

pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

pub fn is_pydantic_basemodel(py: Python, obj: &Bound<'_, PyAny>) -> Result<bool, UtilError> {
    let pydantic = match py.import("pydantic") {
        Ok(module) => module,
        // return false if pydantic cannot be imported
        Err(_) => return Ok(false),
    };

    let basemodel = pydantic.getattr("BaseModel")?;

    // check if context is a pydantic model
    let is_basemodel = obj
        .is_instance(&basemodel)
        .map_err(|e| UtilError::FailedToCheckPydanticModel(e.to_string()))?;

    Ok(is_basemodel)
}

fn process_dict_with_nested_models(
    py: Python<'_>,
    dict: &Bound<'_, PyAny>,
) -> Result<Value, UtilError> {
    let py_dict = dict.cast::<PyDict>()?;
    let mut result = serde_json::Map::new();

    for (key, value) in py_dict.iter() {
        let key_str: String = key.extract()?;
        let processed_value = depythonize_object_to_value(py, &value)?;
        result.insert(key_str, processed_value);
    }

    Ok(Value::Object(result))
}

pub fn depythonize_object_to_value<'py>(
    py: Python<'py>,
    value: &Bound<'py, PyAny>,
) -> Result<Value, UtilError> {
    let py_value = if is_pydantic_basemodel(py, value)? {
        let model = value.call_method0("model_dump")?;
        depythonize(&model)?
    } else if value.is_instance_of::<PyDict>() {
        process_dict_with_nested_models(py, value)?
    } else {
        depythonize(value)?
    };
    Ok(py_value)
}
