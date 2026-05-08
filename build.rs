use std::{env, fs, path::Path};

fn main() {
    let specs = [
        ("occurrence", "specs/occurrence.json"),
        ("checklistbank", "specs/checklistbank.json"),
    ];

    let out_dir = env::var("OUT_DIR").expect("OUT_DIR is set by cargo");
    println!("cargo:rerun-if-changed=build.rs");
    for (_, path) in &specs {
        println!("cargo:rerun-if-changed={path}");
    }

    // typify's type lowering is deeply recursive over the GBIF schemas and
    // overflows the default 2 MiB build-script stack on macOS. Run the work on
    // a dedicated thread with a generous stack.
    let out_dir_owned = out_dir.clone();
    std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            for (name, path) in specs {
                generate(name, path, &out_dir_owned);
            }
        })
        .expect("spawn build thread")
        .join()
        .expect("build thread panicked");
}

fn generate(name: &str, path: &str, out_dir: &str) {
    let raw = fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path}: {e}"));

    let mut value: serde_json::Value =
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse {path}: {e}"));

    // progenitor uses the openapiv3 crate, which targets OpenAPI 3.0.x.
    // The GBIF Occurrence spec advertises 3.1.0 but does not use any 3.1-only
    // features (no webhooks, no nullable type arrays, no jsonSchemaDialect),
    // so it parses cleanly once we relabel the version.
    if let Some(v) = value.get_mut("openapi") {
        if v.as_str() == Some("3.1.0") {
            *v = serde_json::Value::String("3.0.3".into());
        }
    }
    patch_malformed_parameters(&mut value);
    break_predicate_recursion(&mut value);
    stub_missing_schemas(&mut value);
    patch_empty_media_types(&mut value);
    patch_missing_path_params(&mut value);
    collapse_to_single_media_type(&mut value);
    loosen_runtime_polymorphic_schemas(&mut value);

    let spec: openapiv3::OpenAPI =
        serde_json::from_value(value).unwrap_or_else(|e| panic!("decode openapi {path}: {e}"));

    // The default `Positional` interface generates `client.method(arg, arg, ..)`.
    // The GBIF Occurrence search endpoint has 130+ parameters so callers will
    // typically reach for the builder shim wrapper in `lib.rs`, but we keep
    // the positional surface so every operation compiles — `Builder` style
    // panics on a handful of GBIF operations inside progenitor 0.14.
    let mut generator = progenitor::Generator::default();
    let tokens = generator
        .generate_tokens(&spec)
        .unwrap_or_else(|e| panic!("generate {name}: {e}"));

    let ast = syn::parse2(tokens).unwrap_or_else(|e| panic!("syn parse {name}: {e}"));
    let pretty = prettyplease::unparse(&ast);

    let out = Path::new(out_dir).join(format!("{name}.rs"));
    fs::write(&out, pretty).unwrap_or_else(|e| panic!("write {}: {e}", out.display()));
}

/// A handful of GBIF parameters declare neither `schema` nor `content`, which
/// is invalid OpenAPI and rejected by the openapiv3 parser. Inject a permissive
/// `schema: { type: string }` so generation can proceed.
fn patch_malformed_parameters(spec: &mut serde_json::Value) {
    fn fix_param_list(params: &mut serde_json::Value) {
        let Some(list) = params.as_array_mut() else {
            return;
        };
        for p in list {
            let Some(obj) = p.as_object_mut() else {
                continue;
            };
            if obj.contains_key("$ref") {
                continue;
            }
            if !obj.contains_key("schema") && !obj.contains_key("content") {
                obj.insert("schema".into(), serde_json::json!({ "type": "string" }));
            }
        }
    }

    if let Some(paths) = spec.get_mut("paths").and_then(|v| v.as_object_mut()) {
        for item in paths.values_mut() {
            let Some(item_obj) = item.as_object_mut() else {
                continue;
            };
            if let Some(p) = item_obj.get_mut("parameters") {
                fix_param_list(p);
            }
            for method in [
                "get", "post", "put", "delete", "patch", "head", "options", "trace",
            ] {
                if let Some(op) = item_obj.get_mut(method) {
                    if let Some(p) = op.get_mut("parameters") {
                        fix_param_list(p);
                    }
                }
            }
        }
    }

    if let Some(comp_params) = spec
        .get_mut("components")
        .and_then(|c| c.get_mut("parameters"))
        .and_then(|p| p.as_object_mut())
    {
        for p in comp_params.values_mut() {
            let mut wrap = serde_json::Value::Array(vec![p.clone()]);
            fix_param_list(&mut wrap);
            *p = wrap.as_array_mut().unwrap().remove(0);
        }
    }
}

/// The Occurrence download predicate types are mutually and self-recursive
/// (a Conjunction contains Predicates, which include Conjunctions, …). typify
/// cannot flatten that through `allOf` and recurses indefinitely. Replace the
/// recursive bodies with permissive shapes so the sub-predicate fields fall
/// through to `serde_json::Value`. Callers building download requests can
/// still construct predicate trees as untyped JSON.
fn break_predicate_recursion(spec: &mut serde_json::Value) {
    let Some(schemas) = spec
        .get_mut("components")
        .and_then(|c| c.get_mut("schemas"))
        .and_then(|s| s.as_object_mut())
    else {
        return;
    };

    for name in ["ConjunctionPredicate", "DisjunctionPredicate"] {
        if let Some(schema) = schemas.get_mut(name) {
            *schema = serde_json::json!({
                "allOf": [{ "$ref": "#/components/schemas/Predicate" }],
                "type": "object",
                "properties": {
                    "predicates": {
                        "type": "array",
                        "items": {},
                        "description": "The list of sub-predicates to combine."
                    }
                }
            });
        }
    }
    if let Some(schema) = schemas.get_mut("NotPredicate") {
        *schema = serde_json::json!({
            "allOf": [{ "$ref": "#/components/schemas/Predicate" }],
            "type": "object",
            "properties": {
                "predicate": {
                    "description": "The sub-predicate to negate."
                }
            }
        });
    }
}

/// The Occurrence spec references a few component schemas that are not
/// actually defined (CountQuery, paging response wrappers around download
/// usage). Stub them out as untyped objects so generation doesn't abort.
fn stub_missing_schemas(spec: &mut serde_json::Value) {
    use serde_json::json;
    use std::collections::BTreeSet;

    let referenced: BTreeSet<String> = {
        let text = serde_json::to_string(spec).unwrap_or_default();
        let mut out = BTreeSet::new();
        let needle = "\"#/components/schemas/";
        let mut rest = text.as_str();
        while let Some(idx) = rest.find(needle) {
            let after = &rest[idx + needle.len()..];
            if let Some(end) = after.find('"') {
                out.insert(after[..end].to_string());
                rest = &after[end..];
            } else {
                break;
            }
        }
        out
    };

    let components = spec
        .as_object_mut()
        .and_then(|o| {
            o.entry("components")
                .or_insert_with(|| json!({}))
                .as_object_mut()
        })
        .expect("components is an object");
    let schemas = components
        .entry("schemas")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .expect("schemas is an object");

    let defined: BTreeSet<String> = schemas.keys().cloned().collect();
    for name in referenced.difference(&defined) {
        schemas.insert(
            name.clone(),
            json!({
                "type": "object",
                "additionalProperties": true,
                "description": format!("Stub for undefined GBIF schema {name}.")
            }),
        );
    }
}

/// progenitor panics on `MediaType { schema: None, .. }` — a few endpoints in
/// the Occurrence spec declare response content types without any schema body.
/// Inject a permissive `{}` schema so generation produces a `serde_json::Value`
/// (or bytes) response.
fn patch_empty_media_types(spec: &mut serde_json::Value) {
    fn fix_content(content: &mut serde_json::Value) {
        let Some(map) = content.as_object_mut() else {
            return;
        };
        for media in map.values_mut() {
            let Some(obj) = media.as_object_mut() else {
                continue;
            };
            if !obj.contains_key("schema") {
                obj.insert("schema".into(), serde_json::json!({}));
            }
        }
    }

    let Some(paths) = spec.get_mut("paths").and_then(|v| v.as_object_mut()) else {
        return;
    };
    for item in paths.values_mut() {
        let Some(item_obj) = item.as_object_mut() else {
            continue;
        };
        for method in [
            "get", "post", "put", "delete", "patch", "head", "options", "trace",
        ] {
            let Some(op) = item_obj.get_mut(method).and_then(|v| v.as_object_mut()) else {
                continue;
            };
            if let Some(rb) = op.get_mut("requestBody").and_then(|v| v.get_mut("content")) {
                fix_content(rb);
            }
            if let Some(responses) = op.get_mut("responses").and_then(|v| v.as_object_mut()) {
                for resp in responses.values_mut() {
                    if let Some(c) = resp.get_mut("content") {
                        fix_content(c);
                    }
                }
            }
        }
    }
}

/// Some operations reference a `{templateVar}` in the URL but never declare it
/// in `parameters`. progenitor's template renderer panics in that case. Walk
/// each operation, find unresolved path variables, and inject a string-typed
/// path parameter for each missing name.
fn patch_missing_path_params(spec: &mut serde_json::Value) {
    use serde_json::json;

    fn declared_path_params(
        op_params: &[serde_json::Value],
        component_params: &serde_json::Map<String, serde_json::Value>,
    ) -> Vec<String> {
        let mut names = Vec::new();
        for p in op_params {
            if let Some(reference) = p.get("$ref").and_then(|v| v.as_str()) {
                let key = reference.rsplit('/').next().unwrap_or("");
                if let Some(target) = component_params.get(key) {
                    if target.get("in").and_then(|v| v.as_str()) == Some("path") {
                        if let Some(name) = target.get("name").and_then(|v| v.as_str()) {
                            names.push(name.to_string());
                        }
                    }
                }
            } else if p.get("in").and_then(|v| v.as_str()) == Some("path") {
                if let Some(name) = p.get("name").and_then(|v| v.as_str()) {
                    names.push(name.to_string());
                }
            }
        }
        names
    }

    let component_params = spec
        .get("components")
        .and_then(|c| c.get("parameters"))
        .and_then(|p| p.as_object())
        .cloned()
        .unwrap_or_default();

    let Some(paths) = spec.get_mut("paths").and_then(|v| v.as_object_mut()) else {
        return;
    };

    for (path, item) in paths.iter_mut() {
        let template_vars: Vec<String> = path
            .split('{')
            .skip(1)
            .filter_map(|s| s.split('}').next().map(|s| s.to_string()))
            .filter(|s| !s.is_empty())
            .collect();
        if template_vars.is_empty() {
            continue;
        }

        let Some(item_obj) = item.as_object_mut() else {
            continue;
        };
        let shared = item_obj
            .get("parameters")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        for method in [
            "get", "post", "put", "delete", "patch", "head", "options", "trace",
        ] {
            let Some(op) = item_obj.get_mut(method).and_then(|v| v.as_object_mut()) else {
                continue;
            };
            let mut existing = op
                .get("parameters")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            let mut all = existing.clone();
            all.extend(shared.iter().cloned());
            let declared = declared_path_params(&all, &component_params);
            for var in &template_vars {
                if !declared.contains(var) {
                    existing.push(json!({
                        "name": var,
                        "in": "path",
                        "required": true,
                        "schema": { "type": "string" }
                    }));
                }
            }
            op.insert("parameters".into(), serde_json::Value::Array(existing));
        }
    }
}

/// A few schemas in the spec describe a struct shape but the live API returns
/// a primitive (string) or a struct depending on context. The discrepancy is
/// a known GBIF quirk; accept either by widening the schema to untyped JSON.
fn loosen_runtime_polymorphic_schemas(spec: &mut serde_json::Value) {
    const POLYMORPHIC: &[&str] = &["IsoDateInterval"];
    let Some(schemas) = spec
        .get_mut("components")
        .and_then(|c| c.get_mut("schemas"))
        .and_then(|s| s.as_object_mut())
    else {
        return;
    };
    for name in POLYMORPHIC {
        if let Some(schema) = schemas.get_mut(*name) {
            *schema = serde_json::json!({
                "description": format!("{name} (untyped — the GBIF API returns either a struct or a primitive depending on the endpoint).")
            });
        }
    }
}

/// progenitor 0.14 only emits one media type per request body / response. The
/// GBIF specs frequently advertise both `application/json` and JSONP-style
/// `application/x-javascript` (and occasionally `application/octet-stream` or
/// `text/xml`). Collapse each content map to a single entry, preferring JSON.
fn collapse_to_single_media_type(spec: &mut serde_json::Value) {
    fn pick(content: &mut serde_json::Value) {
        let Some(map) = content.as_object_mut() else {
            return;
        };
        if map.len() <= 1 {
            return;
        }
        let preferred = ["application/json", "application/octet-stream", "text/plain"];
        let chosen = preferred
            .iter()
            .find(|p| map.keys().any(|k| k.starts_with(*p)))
            .map(|p| map.keys().find(|k| k.starts_with(*p)).unwrap().clone())
            .or_else(|| map.keys().next().cloned());
        let Some(key) = chosen else { return };
        let value = map.remove(&key).unwrap();
        map.clear();
        map.insert(key, value);
    }

    let Some(paths) = spec.get_mut("paths").and_then(|v| v.as_object_mut()) else {
        return;
    };
    for item in paths.values_mut() {
        let Some(item_obj) = item.as_object_mut() else {
            continue;
        };
        for method in [
            "get", "post", "put", "delete", "patch", "head", "options", "trace",
        ] {
            let Some(op) = item_obj.get_mut(method).and_then(|v| v.as_object_mut()) else {
                continue;
            };
            if let Some(c) = op.get_mut("requestBody").and_then(|v| v.get_mut("content")) {
                pick(c);
            }
            if let Some(responses) = op.get_mut("responses").and_then(|v| v.as_object_mut()) {
                for resp in responses.values_mut() {
                    if let Some(c) = resp.get_mut("content") {
                        pick(c);
                    }
                }
            }
        }
    }
}
