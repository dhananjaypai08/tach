use pyo3::{BoundObject, prelude::*, pybacked::PyBackedStr};
use serde::{Deserialize, Serialize, de::Visitor};
use std::{convert::Infallible, fmt::Display};

const IF_GIT_REPO: &str = "if_git_repo";
const EXPECTED_MSG: &str = "expected `bool | Literal['if_git_repo']`";

#[derive(Debug, Default, Copy, Clone, PartialEq)]
pub enum RespectGitIgnore {
    #[default]
    True,
    False,
    IfGitRepo,
}

impl From<bool> for RespectGitIgnore {
    fn from(value: bool) -> Self {
        if value { Self::True } else { Self::False }
    }
}

impl TryFrom<&str> for RespectGitIgnore {
    type Error = String;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        if value == IF_GIT_REPO {
            Ok(Self::IfGitRepo)
        } else {
            Err(EXPECTED_MSG.to_string())
        }
    }
}

impl Display for RespectGitIgnore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl<'py> IntoPyObject<'py> for RespectGitIgnore {
    type Target = PyAny;
    type Output = Bound<'py, Self::Target>;
    type Error = Infallible;
    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        Ok(match self {
            Self::True => true.into_pyobject(py)?.into_bound().into_any(),
            Self::False => false.into_pyobject(py)?.into_bound().into_any(),
            Self::IfGitRepo => IF_GIT_REPO.into_pyobject(py)?.into_any(),
        })
    }
}

impl<'py> FromPyObject<'py, 'py> for RespectGitIgnore {
    type Error = PyErr;

    fn extract(obj: Borrowed<'py, 'py, PyAny>) -> Result<Self, Self::Error> {
        if let Ok(b) = obj.extract::<bool>() {
            return Ok(b.into());
        }
        if let Ok(s) = obj.extract::<PyBackedStr>() {
            let result: &str = s.as_ref();
            result
                .try_into()
                .map_err(PyErr::new::<pyo3::exceptions::PyTypeError, _>)
        } else {
            Err(PyErr::new::<pyo3::exceptions::PyTypeError, _>(EXPECTED_MSG))
        }
    }
}

impl Serialize for RespectGitIgnore {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::IfGitRepo => serializer.serialize_str(IF_GIT_REPO),
            Self::True => serializer.serialize_bool(true),
            Self::False => serializer.serialize_bool(false),
        }
    }
}

struct RespectGitIgnoreVisitor;

impl<'de> Visitor<'de> for RespectGitIgnoreVisitor {
    type Value = RespectGitIgnore;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str(EXPECTED_MSG)
    }

    fn visit_bool<E: serde::de::Error>(self, value: bool) -> Result<Self::Value, E> {
        Ok(value.into())
    }

    fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Self::Value, E> {
        v.try_into().map_err(E::custom)
    }
}

impl<'de> Deserialize<'de> for RespectGitIgnore {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(RespectGitIgnoreVisitor)
    }
}
