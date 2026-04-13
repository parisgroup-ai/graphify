use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::lang::ExtractionResult;

const CACHE_VERSION: u32 = 1;
