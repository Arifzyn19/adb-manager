//! Minimal binary-Android-XML (AXML) decoder for `AndroidManifest.xml`.
//!
//! Only the structures `aapt` emits for manifests are supported:
//! string pool, resource map (skipped), namespace chunks (skipped) and
//! element start/end. Everything else is skipped by chunk size, so unknown
//! chunks from newer build tools degrade to "missing field" instead of a
//! hard failure.
//!
//! Two input shapes are accepted:
//! - binary AXML (u16 type `0x0003` magic)
//! - plain-text XML (someone decoded it with an external tool) — scraped
//!   with a small attribute scanner, no XML library needed.

use std::collections::HashMap;

/// Structured manifest facts the UI shows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ManifestData {
    pub package: String,
    pub version_name: Option<String>,
    pub version_code: Option<String>,
    pub min_sdk: Option<String>,
    pub target_sdk: Option<String>,
    pub compile_sdk: Option<String>,
    pub app_label: Option<String>,
    pub debuggable: bool,
    pub permissions: Vec<String>,
    pub features: Vec<String>,
    pub activities: Vec<String>,
    pub services: Vec<String>,
    pub receivers: Vec<String>,
    pub providers: Vec<String>,
}

/// Parse either binary AXML or plain-text manifest XML.
pub fn parse_manifest(bytes: &[u8]) -> Result<ManifestData, String> {
    if bytes.len() >= 2 && u16::from_le_bytes([bytes[0], bytes[1]]) == 0x0003 {
        parse_binary(bytes)
    } else if looks_like_text_xml(bytes) {
        Ok(parse_text_fallback(&String::from_utf8_lossy(bytes)))
    } else {
        Err("AndroidManifest.xml is neither binary AXML nor text XML.".to_string())
    }
}

fn looks_like_text_xml(bytes: &[u8]) -> bool {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(256)]);
    let head = head.trim_start_matches('\u{feff}').trim_start();
    head.starts_with("<?xml") || head.starts_with("<manifest")
}

// --- Binary AXML ----------------------------------------------------------

const CHUNK_STRING_POOL: u16 = 0x0001;
const CHUNK_XML_RESOURCE_MAP: u16 = 0x0180;
const CHUNK_XML_START_NAMESPACE: u16 = 0x0100;
const CHUNK_XML_END_NAMESPACE: u16 = 0x0101;
const CHUNK_XML_START_ELEMENT: u16 = 0x0102;
const CHUNK_XML_END_ELEMENT: u16 = 0x0103;
const CHUNK_XML_CDATA: u16 = 0x0104;

const UTF8_FLAG: u32 = 1 << 8;

// Typed-value data types (androidfw/ResourceTypes.h).
const TYPE_STRING: u8 = 0x03;
const TYPE_INT_BOOLEAN: u8 = 0x12;

struct Cursor<'a> {
    b: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(b: &'a [u8]) -> Self {
        Self { b, pos: 0 }
    }
    fn remaining(&self) -> usize {
        self.b.len().saturating_sub(self.pos)
    }
    fn u16(&mut self) -> Result<u16, String> {
        if self.remaining() < 2 {
            return Err("truncated AXML (u16)".to_string());
        }
        let v = u16::from_le_bytes([self.b[self.pos], self.b[self.pos + 1]]);
        self.pos += 2;
        Ok(v)
    }
    fn u32(&mut self) -> Result<u32, String> {
        if self.remaining() < 4 {
            return Err("truncated AXML (u32)".to_string());
        }
        let v = u32::from_le_bytes([
            self.b[self.pos],
            self.b[self.pos + 1],
            self.b[self.pos + 2],
            self.b[self.pos + 3],
        ]);
        self.pos += 4;
        Ok(v)
    }
    fn i32(&mut self) -> Result<i32, String> {
        Ok(self.u32()? as i32)
    }
    fn bytes(&mut self, n: usize) -> Result<&'a [u8], String> {
        if self.remaining() < n {
            return Err("truncated AXML (bytes)".to_string());
        }
        let s = &self.b[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }
    fn seek(&mut self, pos: usize) -> Result<(), String> {
        if pos > self.b.len() {
            return Err("AXML seek out of range".to_string());
        }
        self.pos = pos;
        Ok(())
    }
}

/// Variable-length length prefix used by string pools.
/// UTF-16 pool: u16 units; UTF-8 pool: u8 units (high bit → 2-byte form).
fn read_len16(c: &mut Cursor<'_>) -> Result<usize, String> {
    let v = c.u16()?;
    if v & 0x8000 != 0 {
        let lo = c.u16()? as usize;
        Ok((((v & 0x7FFF) as usize) << 16) | lo)
    } else {
        Ok(v as usize)
    }
}

fn read_len8(c: &mut Cursor<'_>) -> Result<usize, String> {
    if c.remaining() < 1 {
        return Err("truncated AXML string length".to_string());
    }
    let b0 = c.b[c.pos];
    c.pos += 1;
    if b0 & 0x80 != 0 {
        if c.remaining() < 1 {
            return Err("truncated AXML string length".to_string());
        }
        let b1 = c.b[c.pos];
        c.pos += 1;
        Ok((((b0 & 0x7F) as usize) << 8) | b1 as usize)
    } else {
        Ok(b0 as usize)
    }
}

fn decode_pool(bytes: &[u8], chunk_off: usize, chunk_size: usize) -> Result<Vec<String>, String> {
    let mut c = Cursor::new(bytes);
    c.seek(chunk_off + 8)?;
    let string_count = c.u32()? as usize;
    let _style_count = c.u32()?;
    let flags = c.u32()?;
    let strings_start = c.u32()? as usize;
    let _styles_start = c.u32()?;
    if string_count > 50_000 {
        return Err("absurd string-pool count".to_string());
    }
    let mut offsets = Vec::with_capacity(string_count);
    for _ in 0..string_count {
        offsets.push(c.u32()? as usize);
    }
    let utf8 = flags & UTF8_FLAG != 0;
    let mut out = Vec::with_capacity(string_count);
    for off in offsets {
        let abs = chunk_off + strings_start + off;
        if abs >= bytes.len() {
            out.push(String::new());
            continue;
        }
        let mut sc = Cursor::new(bytes);
        sc.seek(abs)?;
        let s = if utf8 {
            // char count (ignored) + byte count + raw UTF-8 bytes.
            let _chars = read_len16(&mut sc)?;
            let n = read_len8(&mut sc)?;
            let raw = sc.bytes(n.min(sc.remaining()))?;
            String::from_utf8_lossy(raw).into_owned()
        } else {
            let n = read_len16(&mut sc)?;
            let mut v = Vec::with_capacity(n.min(512));
            for _ in 0..n {
                match sc.u16() {
                    Ok(u) => v.push(u),
                    Err(_) => break,
                }
            }
            String::from_utf16_lossy(&v)
        };
        out.push(s);
    }
    let _ = chunk_size;
    Ok(out)
}

fn pool_get(pool: &[String], idx: i32) -> String {
    if idx < 0 {
        return String::new();
    }
    pool.get(idx as usize).cloned().unwrap_or_default()
}

fn typed_to_string(pool: &[String], data_type: u8, data: u32) -> String {
    match data_type {
        TYPE_STRING => pool.get(data as usize).cloned().unwrap_or_default(),
        TYPE_INT_BOOLEAN => {
            if data != 0 {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        0x10..=0x1F => (data as i32).to_string(),
        0x01 => format!("@0x{:08X}", data),
        0x04 => format!("{:.4}", f32::from_bits(data)),
        _ => format!("0x{:08X}", data),
    }
}

fn parse_binary(bytes: &[u8]) -> Result<ManifestData, String> {
    let mut c = Cursor::new(bytes);
    if c.u16()? != 0x0003 || c.u16()? != 8 {
        return Err("not an AXML file".to_string());
    }
    let file_size = c.u32()? as usize;
    if file_size > bytes.len() + 8 || file_size < 8 {
        // Tolerate trailing garbage; only need chunks within the buffer.
    }

    let mut pool: Vec<String> = Vec::new();
    // Element stack + per-element collected attributes (name → value).
    let mut stack: Vec<String> = Vec::new();
    let mut attr_stack: Vec<HashMap<String, String>> = Vec::new();
    let mut out = ManifestData::default();
    // Parallel stack for application/android:name resolution context.
    let mut app_name_stack: Vec<bool> = Vec::new();

    let mut off = 8usize;
    while off + 8 <= bytes.len() {
        let mut hc = Cursor::new(bytes);
        hc.seek(off)?;
        let chunk_type = hc.u16()?;
        let _header_size = hc.u16()?;
        let chunk_size = hc.u32()? as usize;
        if chunk_size < 8 || off + chunk_size > bytes.len() + 1 {
            break; // trailing padding / corruption: keep what we decoded.
        }
        match chunk_type {
            CHUNK_STRING_POOL => {
                pool = decode_pool(bytes, off, chunk_size)?;
            }
            CHUNK_XML_RESOURCE_MAP | CHUNK_XML_CDATA => {}
            CHUNK_XML_START_NAMESPACE | CHUNK_XML_END_NAMESPACE => {}
            CHUNK_XML_START_ELEMENT => {
                let mut ec = Cursor::new(bytes);
                ec.seek(off + 8)?;
                let _line = ec.u32()?;
                let _comment = ec.i32()?;
                let _ns = ec.i32()?;
                let name_idx = ec.i32()?;
                let attr_start = ec.u16()? as usize;
                let attr_size = ec.u16()? as usize;
                let attr_count = ec.u16()? as usize;
                let _id_idx = ec.u16()?;
                let _class_idx = ec.u16()?;
                let _style_idx = ec.u16()?;
                let name = pool_get(&pool, name_idx);
                let mut attrs: HashMap<String, String> = HashMap::new();
                if attr_size >= 20 && attr_count <= 128 {
                    // attributeStart is relative to the chunk start.
                    let base = off.saturating_add(attr_start);
                    for i in 0..attr_count {
                        let mut ac = Cursor::new(bytes);
                        if ac.seek(base + i * attr_size).is_err() {
                            break;
                        }
                        let _a_ns = ac.i32()?;
                        let a_name_idx = ac.i32()?;
                        let _raw = ac.i32()?;
                        let _vsize = ac.u16()?;
                        let _res0 = if ac.remaining() >= 1 {
                            let v = ac.b[ac.pos];
                            ac.pos += 1;
                            v
                        } else {
                            0
                        };
                        let dtype = if ac.remaining() >= 1 {
                            let v = ac.b[ac.pos];
                            ac.pos += 1;
                            v
                        } else {
                            0
                        };
                        let data = ac.u32().unwrap_or(0);
                        let a_name = pool_get(&pool, a_name_idx);
                        if !a_name.is_empty() {
                            attrs.insert(a_name, typed_to_string(&pool, dtype, data));
                        }
                    }
                }
                let parent = stack.last().cloned().unwrap_or_default();
                match name.as_str() {
                    "manifest" => {
                        out.package = attrs.get("package").cloned().unwrap_or_default();
                        out.version_code = attrs.get("versionCode").cloned();
                        out.version_name = attrs.get("versionName").cloned();
                        out.compile_sdk = attrs.get("compileSdkVersion").cloned();
                    }
                    "uses-sdk" => {
                        if let Some(v) = attrs.get("minSdkVersion") {
                            out.min_sdk = Some(v.clone());
                        }
                        if let Some(v) = attrs.get("targetSdkVersion") {
                            out.target_sdk = Some(v.clone());
                        }
                    }
                    "uses-permission" | "uses-permission-sdk-23" => {
                        if let Some(p) = attrs.get("name") {
                            if !p.is_empty() && !out.permissions.contains(p) {
                                out.permissions.push(p.clone());
                            }
                        }
                    }
                    "uses-feature" => {
                        if let Some(f) = attrs.get("name") {
                            if !f.is_empty() && !out.features.contains(f) {
                                out.features.push(f.clone());
                            }
                        }
                    }
                    "application" => {
                        if let Some(l) = attrs.get("label") {
                            out.app_label = Some(l.clone());
                        }
                        out.debuggable = attrs.get("debuggable").is_some_and(|v| v == "true");
                    }
                    "activity" | "activity-alias" if parent == "application" => {
                        if let Some(a) = attrs.get("name") {
                            out.activities.push(qualify(&out.package, a));
                        }
                    }
                    "service" if parent == "application" => {
                        if let Some(a) = attrs.get("name") {
                            out.services.push(qualify(&out.package, a));
                        }
                    }
                    "receiver" if parent == "application" => {
                        if let Some(a) = attrs.get("name") {
                            out.receivers.push(qualify(&out.package, a));
                        }
                    }
                    "provider" if parent == "application" => {
                        if let Some(a) = attrs.get("name") {
                            out.providers.push(qualify(&out.package, a));
                        }
                    }
                    _ => {}
                }
                stack.push(name.clone());
                attr_stack.push(attrs);
                app_name_stack.push(name == "application");
            }
            CHUNK_XML_END_ELEMENT => {
                stack.pop();
                attr_stack.pop();
                app_name_stack.pop();
            }
            _ => {
                // Unknown chunk (e.g. from newer build tools): skip by size.
            }
        }
        if chunk_size == 0 {
            break;
        }
        off += chunk_size;
    }

    if out.package.is_empty() {
        return Err("manifest element not found in AXML".to_string());
    }
    Ok(out)
}

/// `android:name` may be `.Main`, `Main` or fully qualified.
fn qualify(package: &str, name: &str) -> String {
    if name.starts_with('.') {
        format!("{package}{name}")
    } else if name.contains('.') || package.is_empty() {
        name.to_string()
    } else {
        format!("{package}.{name}")
    }
}

// --- Plain-text fallback ---------------------------------------------------

/// Attribute scraper for text manifests (external-tool output).
/// Finds `name="value"` inside the first `<tag ...>` occurrence.
fn tag_attr(xml: &str, tag: &str, attr: &str) -> Option<String> {
    let mut search = 0;
    while let Some(start) = xml[search..].find(&format!("<{tag}")) {
        let abs = search + start;
        let end = xml[abs..].find('>')? + abs;
        let head = &xml[abs..end];
        // Avoid matching <application> when looking for <activity>, etc:
        // require the char after the tag name to be whitespace, '/' or '>'.
        let after = head[tag.len() + 1..].chars().next().unwrap_or('>');
        if !(after.is_whitespace() || after == '/' || after == '>') {
            search = abs + 1;
            continue;
        }
        let needle = format!("{attr}=\"");
        if let Some(vs) = head.find(&needle) {
            let vs = vs + needle.len();
            let ve = head[vs..].find('"')? + vs;
            // Strip android: prefix equivalence — callers pass local names and
            // the text form uses android:name; also accept bare names.
            return Some(head[vs..ve].to_string());
        }
        search = end + 1;
    }
    None
}

fn all_tag_attrs(xml: &str, tag: &str, attr_suffix: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut search = 0;
    while let Some(start) = xml[search..].find(&format!("<{tag}")) {
        let abs = search + start;
        let Some(end_rel) = xml[abs..].find('>') else {
            break;
        };
        let end = abs + end_rel;
        let head = &xml[abs..end];
        let after = head[tag.len() + 1..].chars().next().unwrap_or('>');
        if !(after.is_whitespace() || after == '/' || after == '>') {
            search = abs + 1;
            continue;
        }
        for needle in [
            format!("android:{attr_suffix}=\""),
            format!("{attr_suffix}=\""),
        ] {
            if let Some(vs) = head.find(&needle) {
                let vs = vs + needle.len();
                if let Some(ve) = head[vs..].find('"') {
                    let v = head[vs..vs + ve].to_string();
                    if !out.contains(&v) {
                        out.push(v);
                    }
                    break;
                }
            }
        }
        search = end + 1;
    }
    out
}

fn parse_text_fallback(xml: &str) -> ManifestData {
    let package = tag_attr(xml, "manifest", "package").unwrap_or_default();
    let mut d = ManifestData {
        package: package.clone(),
        version_name: tag_attr(xml, "manifest", "android:versionName")
            .or_else(|| tag_attr(xml, "manifest", "versionName")),
        version_code: tag_attr(xml, "manifest", "android:versionCode")
            .or_else(|| tag_attr(xml, "manifest", "versionCode")),
        ..Default::default()
    };
    let sdk_block = {
        let mut block = String::new();
        if let Some(start) = xml.find("<uses-sdk") {
            if let Some(end) = xml[start..].find('>') {
                block = xml[start..start + end].to_string();
            }
        }
        block
    };
    for needle in ["android:minSdkVersion=\"", "minSdkVersion=\""] {
        if let Some(vs) = sdk_block.find(needle) {
            let vs = vs + needle.len();
            if let Some(ve) = sdk_block[vs..].find('"') {
                d.min_sdk = Some(sdk_block[vs..vs + ve].to_string());
                break;
            }
        }
    }
    for needle in ["android:targetSdkVersion=\"", "targetSdkVersion=\""] {
        if let Some(vs) = sdk_block.find(needle) {
            let vs = vs + needle.len();
            if let Some(ve) = sdk_block[vs..].find('"') {
                d.target_sdk = Some(sdk_block[vs..vs + ve].to_string());
                break;
            }
        }
    }
    // application label / debuggable come from the <application ...> head.
    if let Some(start) = xml.find("<application") {
        let end = xml[start..]
            .find('>')
            .map(|e| start + e)
            .unwrap_or(xml.len());
        let head = &xml[start..end];
        if let Some(vs) = head
            .find("android:label=\"")
            .map(|i| i + "android:label=\"".len())
        {
            if let Some(ve) = head[vs..].find('"') {
                d.app_label = Some(head[vs..vs + ve].to_string());
            }
        }
        d.debuggable = head.contains("android:debuggable=\"true\"");
    }
    d.permissions = {
        let mut v = all_tag_attrs(xml, "uses-permission", "name");
        v.extend(all_tag_attrs(xml, "uses-permission-sdk-23", "name"));
        v.sort();
        v.dedup();
        v
    };
    d.features = all_tag_attrs(xml, "uses-feature", "name");
    // Component names only inside <application>…</application>.
    let app_body = xml
        .find("<application")
        .and_then(|s| {
            xml[s..]
                .find("</application>")
                .map(|e| xml[s..s + e].to_string())
        })
        .unwrap_or_default();
    for (tag, slot) in [
        ("activity", &mut d.activities),
        ("service", &mut d.services),
        ("receiver", &mut d.receivers),
        ("provider", &mut d.providers),
    ] {
        for n in all_tag_attrs(&app_body, tag, "name") {
            slot.push(qualify(&package, &n));
        }
        // <activity-alias> counts as an activity entry point.
        if tag == "activity" {
            for n in all_tag_attrs(&app_body, "activity-alias", "name") {
                slot.push(qualify(&package, &n));
            }
        }
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a minimal binary manifest: pool ["manifest","package","com.x",
    /// "versionName","1.0","uses-permission","name","android.permission.INTERNET"]
    /// plus a manifest element with package and versionName and one
    /// uses-permission child.
    /// String-pool indices are resolved in the test body; see below.
    fn utf16_len_prefixed(s: &str) -> Vec<u8> {
        let u: Vec<u16> = s.encode_utf16().collect();
        let mut v = vec![
            (u.len() as u16).to_le_bytes()[0],
            (u.len() as u16).to_le_bytes()[1],
        ];
        for c in u {
            v.extend_from_slice(&c.to_le_bytes());
        }
        v.extend_from_slice(&[0, 0]);
        v
    }

    fn build_axml(pool: &[&str], chunks: &[Vec<u8>]) -> Vec<u8> {
        let mut strings = Vec::new();
        let mut offsets = Vec::new();
        let mut off = 0usize;
        for s in pool {
            offsets.push(off as u32);
            let enc = utf16_len_prefixed(s);
            off += enc.len();
            strings.extend(enc);
        }
        // pad to 4 bytes (aapt aligns the pool).
        while strings.len() % 4 != 0 {
            strings.push(0);
        }
        let mut pool_chunk = Vec::new();
        pool_chunk.extend_from_slice(&CHUNK_STRING_POOL.to_le_bytes());
        pool_chunk.extend_from_slice(&28u16.to_le_bytes()); // header size
        let pool_size = (28 + pool.len() * 4 + strings.len()) as u32;
        pool_chunk.extend_from_slice(&pool_size.to_le_bytes());
        pool_chunk.extend_from_slice(&(pool.len() as u32).to_le_bytes());
        pool_chunk.extend_from_slice(&0u32.to_le_bytes()); // styles
        pool_chunk.extend_from_slice(&0u32.to_le_bytes()); // flags: UTF-16
        pool_chunk.extend_from_slice(&(28 + pool.len() as u32 * 4).to_le_bytes());
        pool_chunk.extend_from_slice(&0u32.to_le_bytes()); // stylesStart
        for o in &offsets {
            pool_chunk.extend_from_slice(&o.to_le_bytes());
        }
        pool_chunk.extend_from_slice(&strings);

        let mut file = vec![0x03, 0x00, 0x08, 0x00];
        let total: u32 =
            (8 + pool_chunk.len() + chunks.iter().map(|c| c.len()).sum::<usize>()) as u32;
        file.extend_from_slice(&total.to_le_bytes());
        file.extend_from_slice(&pool_chunk);
        for c in chunks {
            file.extend_from_slice(c);
        }
        file
    }

    fn start_element(
        pool_idx: &std::collections::HashMap<&str, u32>,
        name: &str,
        attrs: &[(&str, u8, u32)],
    ) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&CHUNK_XML_START_ELEMENT.to_le_bytes());
        v.extend_from_slice(&16u16.to_le_bytes());
        let size = (36 + attrs.len() * 20) as u32;
        v.extend_from_slice(&size.to_le_bytes());
        v.extend_from_slice(&1u32.to_le_bytes()); // line
        v.extend_from_slice(&0xFFFFFFFFu32.to_le_bytes()); // comment
        v.extend_from_slice(&0xFFFFFFFFu32.to_le_bytes()); // ns
        v.extend_from_slice(&pool_idx[name].to_le_bytes());
        v.extend_from_slice(&36u16.to_le_bytes()); // attrStart
        v.extend_from_slice(&20u16.to_le_bytes()); // attrSize
        v.extend_from_slice(&(attrs.len() as u16).to_le_bytes());
        v.extend_from_slice(&0u16.to_le_bytes()); // id
        v.extend_from_slice(&0u16.to_le_bytes()); // class
        v.extend_from_slice(&0u16.to_le_bytes()); // style
        for (aname, dtype, data) in attrs {
            v.extend_from_slice(&0xFFFFFFFFu32.to_le_bytes()); // ns
            v.extend_from_slice(&pool_idx[aname].to_le_bytes());
            v.extend_from_slice(&0xFFFFFFFFu32.to_le_bytes()); // rawValue
            v.extend_from_slice(&8u16.to_le_bytes()); // value size
            v.push(0); // res0
            v.push(*dtype); // dataType
            v.extend_from_slice(&data.to_le_bytes());
        }
        v
    }

    fn end_element(pool_idx: &std::collections::HashMap<&str, u32>, name: &str) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&CHUNK_XML_END_ELEMENT.to_le_bytes());
        v.extend_from_slice(&16u16.to_le_bytes());
        v.extend_from_slice(&24u32.to_le_bytes());
        v.extend_from_slice(&1u32.to_le_bytes());
        v.extend_from_slice(&0xFFFFFFFFu32.to_le_bytes());
        v.extend_from_slice(&0xFFFFFFFFu32.to_le_bytes());
        v.extend_from_slice(&pool_idx[name].to_le_bytes());
        v
    }

    #[test]
    fn parses_minimal_binary_manifest() {
        let pool = [
            "manifest",
            "package",
            "com.example.app",
            "versionName",
            "2.1",
            "uses-permission",
            "name",
            "android.permission.INTERNET",
        ];
        let map: std::collections::HashMap<&str, u32> = pool
            .iter()
            .enumerate()
            .map(|(i, s)| (*s, i as u32))
            .collect();
        let chunks = vec![
            start_element(
                &map,
                "manifest",
                &[("package", TYPE_STRING, 2), ("versionName", TYPE_STRING, 4)],
            ),
            start_element(&map, "uses-permission", &[("name", TYPE_STRING, 7)]),
            end_element(&map, "uses-permission"),
            end_element(&map, "manifest"),
        ];
        let bytes = build_axml(&pool, &chunks);
        let m = parse_manifest(&bytes).expect("parses");
        assert_eq!(m.package, "com.example.app");
        assert_eq!(m.version_name.as_deref(), Some("2.1"));
        assert_eq!(
            m.permissions,
            vec!["android.permission.INTERNET".to_string()]
        );
    }

    #[test]
    fn binary_manifest_missing_package_errors() {
        let pool = ["manifest"];
        let map: std::collections::HashMap<&str, u32> = pool
            .iter()
            .enumerate()
            .map(|(i, s)| (*s, i as u32))
            .collect();
        let chunks = vec![
            start_element(&map, "manifest", &[]),
            end_element(&map, "manifest"),
        ];
        let bytes = build_axml(&pool, &chunks);
        assert!(parse_manifest(&bytes).is_err());
    }

    #[test]
    fn parses_text_manifest_fallback() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<manifest package="com.example.app" android:versionCode="42" android:versionName="2.1">
  <uses-sdk android:minSdkVersion="26" android:targetSdkVersion="34" />
  <uses-permission android:name="android.permission.INTERNET" />
  <application android:label="Example" android:debuggable="true">
    <activity android:name=".MainActivity" />
    <service android:name="com.example.app.Sync" />
  </application>
</manifest>"#;
        let m = parse_manifest(xml.as_bytes()).expect("parses text");
        assert_eq!(m.package, "com.example.app");
        assert_eq!(m.version_code.as_deref(), Some("42"));
        assert_eq!(m.min_sdk.as_deref(), Some("26"));
        assert_eq!(m.target_sdk.as_deref(), Some("34"));
        assert_eq!(
            m.activities,
            vec!["com.example.app.MainActivity".to_string()]
        );
        assert!(m.debuggable);
    }

    #[test]
    fn garbage_is_rejected() {
        assert!(parse_manifest(b"PK\x03\x04 not xml at all \x00\x01").is_err());
        assert!(parse_manifest(b"").is_err());
    }

    #[test]
    fn qualify_handles_shorthand_names() {
        assert_eq!(qualify("com.x", ".Main"), "com.x.Main");
        assert_eq!(qualify("com.x", "Main"), "com.x.Main");
        assert_eq!(qualify("com.x", "com.y.Other"), "com.y.Other");
    }
}
