use std::fmt;
use std::fs;

const METERS_PER_DEG_LON: f64 = 111_320.0;
const METERS_PER_DEG_LAT: f64 = 110_540.0;
const EARTH_RADIUS_M: f64 = 6_371_000.0;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Point {
    lat: f64,
    lon: f64,
    ele: f64,
}

#[derive(Debug)]
pub struct Error {
    pub code: i32,
    message: String,
}

impl Error {
    fn new(code: i32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Error {}

pub fn run(args: &[String]) -> Result<String, Error> {
    let (input, epsilon, output) = parse_args(args)?;
    let xml = fs::read_to_string(&input)
        .map_err(|err| Error::new(1, format!("failed to read {input}: {err}")))?;
    let points = parse_gpx(&xml);
    if points.len() < 2 {
        return Err(Error::new(2, "fewer than 2 points"));
    }
    let distance = total_distance(&points);
    let gain = elevation_gain(&points);
    let kept = douglas_peucker(&points, &project(&points), epsilon);
    let gpx = write_gpx(&kept);
    fs::write(&output, gpx)
        .map_err(|err| Error::new(1, format!("failed to write {output}: {err}")))?;
    Ok(format!(
        "distance_m {distance:.1}\nelevation_gain_m {gain:.1}\n"
    ))
}

fn parse_args(args: &[String]) -> Result<(String, f64, String), Error> {
    let mut input = None;
    let mut output = None;
    let mut epsilon = None;
    let mut i = 0;
    while i < args.len() {
        let flag = args[i].as_str();
        if !matches!(flag, "--input" | "--output" | "--epsilon") {
            return Err(Error::new(1, format!("unknown argument: {flag}")));
        }
        let Some(value) = args.get(i + 1) else {
            return Err(Error::new(1, format!("missing value for {flag}")));
        };
        match flag {
            "--input" => input = Some(value.clone()),
            "--output" => output = Some(value.clone()),
            "--epsilon" => {
                let parsed = value
                    .trim()
                    .parse::<f64>()
                    .map_err(|_| Error::new(1, "invalid --epsilon"))?;
                if !parsed.is_finite() {
                    return Err(Error::new(1, "invalid --epsilon"));
                }
                epsilon = Some(parsed);
            }
            _ => unreachable!(),
        }
        i += 2;
    }
    match (input, epsilon, output) {
        (Some(input), Some(epsilon), Some(output)) => Ok((input, epsilon, output)),
        _ => Err(Error::new(
            1,
            "usage: gpx-trim --input in.gpx --epsilon 10 --output out.gpx",
        )),
    }
}

struct Tag<'a> {
    name: &'a str,
    raw: &'a str,
    end: usize,
    closing: bool,
    self_closing: bool,
}

fn next_tag(xml: &str, from: usize) -> Option<Tag<'_>> {
    let bytes = xml.as_bytes();
    let mut i = from;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        if let Some(skipped) = skip_markup(xml, i) {
            i = skipped;
            continue;
        }
        let end_bracket = find_tag_end(xml, i)?;
        let closing = bytes.get(i + 1) == Some(&b'/');
        let name_start = if closing { i + 2 } else { i + 1 };
        if name_start > end_bracket {
            i = end_bracket + 1;
            continue;
        }
        let interior = &xml[name_start..end_bracket];
        let self_closing = !closing && interior.trim_end().ends_with('/');
        let token_len = interior
            .find(|c: char| c.is_whitespace() || c == '/' || c == '>')
            .unwrap_or(interior.len());
        let raw_name = &interior[..token_len];
        if raw_name.is_empty() {
            i = end_bracket + 1;
            continue;
        }
        let local = raw_name.rsplit(':').next().unwrap_or(raw_name);
        return Some(Tag {
            name: local,
            raw: &xml[i..=end_bracket],
            end: end_bracket + 1,
            closing,
            self_closing,
        });
    }
    None
}

fn skip_markup(xml: &str, i: usize) -> Option<usize> {
    let rest = xml.get(i..)?;
    if let Some(body) = rest.strip_prefix("<!--") {
        return body.find("-->").map(|p| i + 4 + p + 3);
    }
    if let Some(body) = rest.strip_prefix("<![CDATA[") {
        return body.find("]]>").map(|p| i + 9 + p + 3);
    }
    if let Some(body) = rest.strip_prefix("<?") {
        return body.find("?>").map(|p| i + 2 + p + 2);
    }
    if let Some(body) = rest.strip_prefix("<!") {
        return body.find('>').map(|p| i + 2 + p + 1);
    }
    None
}

fn find_tag_end(xml: &str, open: usize) -> Option<usize> {
    let bytes = xml.as_bytes();
    let mut i = open + 1;
    let mut quote = None;
    while i < bytes.len() {
        let c = bytes[i];
        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
        } else if c == b'"' || c == b'\'' {
            quote = Some(c);
        } else if c == b'>' {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn attr_value<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let mut rest = tag.trim().strip_prefix('<')?;
    if let Some(stripped) = rest.strip_suffix('>') {
        rest = stripped;
    }
    rest = rest.trim_end();
    if let Some(stripped) = rest.strip_suffix('/') {
        rest = stripped.trim_end();
    }
    if let Some(stripped) = rest.strip_prefix('/') {
        rest = stripped;
    }
    let name_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    rest = rest[name_end..].trim_start();
    while !rest.is_empty() {
        let eq = rest.find('=')?;
        let key = rest[..eq].trim();
        if key.is_empty() {
            return None;
        }
        rest = rest[eq + 1..].trim_start();
        let quote = rest.chars().next()?;
        if quote != '"' && quote != '\'' {
            return None;
        }
        let quote_len = quote.len_utf8();
        rest = &rest[quote_len..];
        let end = rest.find(quote)?;
        let value = &rest[..end];
        rest = rest[end + quote_len..].trim_start();
        let local = key.rsplit(':').next().unwrap_or(key);
        if local == name {
            return Some(value.trim());
        }
    }
    None
}

fn attr_f64(tag: &str, name: &str) -> Option<f64> {
    let value = attr_value(tag, name)?;
    let parsed = value.parse::<f64>().ok()?;
    parsed.is_finite().then_some(parsed)
}

fn read_simple_ele(xml: &str, open_end: usize) -> Option<(f64, usize)> {
    let close = next_tag(xml, open_end)?;
    if !(close.closing && close.name == "ele") {
        return None;
    }
    let value = xml[open_end..close.start()].trim().parse::<f64>().ok()?;
    value.is_finite().then_some((value, close.end))
}

impl Tag<'_> {
    fn start(&self) -> usize {
        self.end - self.raw.len()
    }
}

fn parse_gpx(xml: &str) -> Vec<Point> {
    let mut points = Vec::new();
    let mut i = 0;
    while i < xml.len() {
        let Some(tag) = next_tag(xml, i) else {
            break;
        };
        if tag.end <= i {
            break;
        }
        if tag.closing || tag.name != "trkpt" {
            i = tag.end;
            continue;
        }
        match take_trkpt(xml, &tag) {
            Some((point, next)) => {
                if let Some(point) = point {
                    points.push(point);
                }
                i = next.max(tag.end);
            }
            None => i = xml.len(),
        }
    }
    points
}

fn take_trkpt(xml: &str, open: &Tag<'_>) -> Option<(Option<Point>, usize)> {
    let lat = attr_f64(open.raw, "lat");
    let lon = attr_f64(open.raw, "lon");
    if open.self_closing {
        return Some((None, open.end));
    }
    let mut i = open.end;
    let mut depth = 1;
    let mut ele = None;
    let mut saw_direct_ele = false;
    while depth > 0 {
        let tag = next_tag(xml, i)?;
        if tag.end <= i {
            return None;
        }
        if tag.closing {
            depth -= 1;
            i = tag.end;
            if depth == 0 {
                break;
            }
            continue;
        }
        if tag.name == "ele" && depth == 1 && !saw_direct_ele {
            saw_direct_ele = true;
            if let Some((value, next)) = read_simple_ele(xml, tag.end) {
                ele = Some(value);
                i = next;
                continue;
            }
        }
        if !tag.self_closing {
            depth += 1;
        }
        i = tag.end;
    }
    let point = match (lat, lon, ele) {
        (Some(lat), Some(lon), Some(ele)) => Some(Point { lat, lon, ele }),
        _ => None,
    };
    Some((point, i))
}

fn write_gpx(points: &[Point]) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n\
         <gpx version=\"1.1\" xmlns=\"http://www.topografix.com/GPX/1/1\">\n\
         <trk><trkseg>\n",
    );
    for point in points {
        out.push_str(&format!(
            "<trkpt lat=\"{:.6}\" lon=\"{:.6}\"><ele>{:.1}</ele></trkpt>\n",
            point.lat, point.lon, point.ele
        ));
    }
    out.push_str("</trkseg></trk>\n</gpx>\n");
    out
}

/// Equirectangular meters around the first point.
/// `x = (lon-lon0) * 111_320 * cos(lat0)`, `y = (lat-lat0) * 110_540`.
/// Degrees to radians for cos only.
fn project(points: &[Point]) -> Vec<(f64, f64)> {
    let Some(origin) = points.first() else {
        return Vec::new();
    };
    let cos_lat0 = origin.lat.to_radians().cos();
    points
        .iter()
        .map(|point| {
            let x = (point.lon - origin.lon) * METERS_PER_DEG_LON * cos_lat0;
            let y = (point.lat - origin.lat) * METERS_PER_DEG_LAT;
            (x, y)
        })
        .collect()
}

fn perpendicular_distance(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (px, py) = p;
    let (ax, ay) = a;
    let (bx, by) = b;
    let dx = bx - ax;
    let dy = by - ay;
    let len_sq = dx * dx + dy * dy;
    if len_sq == 0.0 {
        let ex = px - ax;
        let ey = py - ay;
        return (ex * ex + ey * ey).sqrt();
    }
    let cross = dx * (ay - py) - dy * (ax - px);
    cross.abs() / len_sq.sqrt()
}

fn douglas_peucker(points: &[Point], xy: &[(f64, f64)], epsilon: f64) -> Vec<Point> {
    if points.len() < 2 {
        return points.to_vec();
    }
    let n = points.len();
    let mut keep = vec![false; n];
    keep[0] = true;
    keep[n - 1] = true;
    let mut stack = vec![(0usize, n - 1)];
    while let Some((start, end)) = stack.pop() {
        if end <= start + 1 {
            continue;
        }
        let mut max_dist = 0.0;
        let mut max_idx = start;
        let mut found = false;
        for i in start + 1..end {
            let dist = perpendicular_distance(xy[i], xy[start], xy[end]);
            if !found || dist > max_dist {
                max_dist = dist;
                max_idx = i;
                found = true;
            }
        }
        if found && max_dist > epsilon {
            keep[max_idx] = true;
            stack.push((start, max_idx));
            stack.push((max_idx, end));
        }
    }
    points
        .iter()
        .enumerate()
        .filter(|(i, _)| keep[*i])
        .map(|(_, point)| *point)
        .collect()
}

fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let phi1 = lat1.to_radians();
    let phi2 = lat2.to_radians();
    let d_phi = (lat2 - lat1).to_radians();
    let d_lon = (lon2 - lon1).to_radians();
    let sin_dphi = (d_phi * 0.5).sin();
    let sin_dlon = (d_lon * 0.5).sin();
    let a = sin_dphi * sin_dphi + phi1.cos() * phi2.cos() * sin_dlon * sin_dlon;
    let a = a.clamp(0.0, 1.0);
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
    EARTH_RADIUS_M * c
}

fn total_distance(points: &[Point]) -> f64 {
    points
        .windows(2)
        .map(|pair| haversine_m(pair[0].lat, pair[0].lon, pair[1].lat, pair[1].lon))
        .sum()
}

fn elevation_gain(points: &[Point]) -> f64 {
    points
        .windows(2)
        .map(|pair| pair[1].ele - pair[0].ele)
        .filter(|delta| *delta > 0.0)
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn workspace_tmp(name: &str) -> PathBuf {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-tmp")
            .join(name);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn invoke(name: &str, gpx: &str, epsilon: &str) -> (Result<String, Error>, PathBuf, PathBuf) {
        let dir = workspace_tmp(name);
        let input = dir.join("in.gpx");
        let output = dir.join("out.gpx");
        fs::write(&input, gpx).unwrap();
        let _ = fs::remove_file(&output);
        let args = vec![
            "--input".to_string(),
            input.to_str().unwrap().to_string(),
            "--epsilon".to_string(),
            epsilon.to_string(),
            "--output".to_string(),
            output.to_str().unwrap().to_string(),
        ];
        (run(&args), input, output)
    }

    fn line_gpx() -> String {
        let lat0: f64 = 45.0;
        let lon0: f64 = -120.0;
        let scale = 111_320.0 * lat0.to_radians().cos();
        let lon = |east_m: f64| lon0 + east_m / scale;
        let lat = |north_m: f64| lat0 + north_m / 110_540.0;
        let points = [
            (lat0, lon0, 10.0),
            (lat(5.0), lon(100.0), 80.0),
            (lat0, lon(200.0), 15.0),
        ];
        let mut xml = String::from(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<gpx version=\"1.1\">\n<trk><name>ignore-me</name><trkseg>\n",
        );
        for (lat, lon, ele) in points {
            xml.push_str(&format!(
                "<trkpt lat=\"{lat:?}\" lon=\"{lon:?}\"><time>ignore-me</time><ele>{ele}</ele></trkpt>\n"
            ));
        }
        xml.push_str("</trkseg></trk>\n</gpx>\n");
        xml
    }

    const ELEVATION_GPX: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<gpx version="1.1" creator="fixture">
  <metadata><name>ignore-me</name><ele>9999</ele></metadata>
  <!-- <trkpt lat="9" lon="9"><ele>1000</ele></trkpt> -->
  <wpt lat="1" lon="1"><ele>5000</ele></wpt>
  <trk>
    <name>ignore-me</name>
    <trkseg>
      <trkpt lat="0" lon="0"><ele>100.0</ele><time>2020-01-01T00:00:00Z</time></trkpt>
      <trkpt lon="0.01" lat="0"><ele> 140.0 </ele></trkpt>
      <wpt lat="0" lon="0.015"><ele>5000</ele></wpt>
      <trkpt lat="0" lon="0.02"><ele>130.0</ele></trkpt>
      <trkpt lat="0" lon="0.025"></trkpt>
      <trkpt lat="0" lon="0.03"><ele>155.5</ele></trkpt>
      <trkpt lat="0" lon="0.04"><ele>nope</ele></trkpt>
      <trkpt lat="5" lon="5"/>
    </trkseg>
  </trk>
  <rte><rtept lat="2" lon="2"><ele>8000</ele></rtept></rte>
</gpx>
"#;

    fn assert_precision(xml: &str) {
        let mut rest = xml;
        let mut points = 0;
        while let Some(idx) = rest.find("<trkpt ") {
            rest = &rest[idx + 7..];
            let end = rest.find('>').unwrap();
            let tag = &rest[..end];
            for key in ["lat", "lon"] {
                let needle = format!("{key}=\"");
                let start = tag.find(&needle).unwrap() + needle.len();
                let value = &tag[start..tag[start..].find('"').unwrap() + start];
                let frac = value.split('.').nth(1).expect(value);
                assert_eq!(frac.len(), 6, "{key}={value}");
            }
            let ele_rel = rest.find("<ele>").unwrap();
            let ele_start = ele_rel + 5;
            let ele_end = rest.find("</ele>").unwrap();
            let ele = &rest[ele_start..ele_end];
            let frac = ele.split('.').nth(1).expect(ele);
            assert_eq!(frac.len(), 1, "ele={ele}");
            points += 1;
            rest = &rest[ele_end..];
        }
        assert!(points >= 2, "expected kept points, got {points}");
    }

    #[test]
    fn projection_uses_spec_constants() {
        let points = [
            Point {
                lat: 45.0,
                lon: 10.0,
                ele: 0.0,
            },
            Point {
                lat: 46.0,
                lon: 11.0,
                ele: 0.0,
            },
        ];
        let xy = project(&points);
        assert_eq!(xy[0], (0.0, 0.0));
        let expected_x = (11.0 - 10.0) * 111_320.0 * 45.0_f64.to_radians().cos();
        let expected_y = (46.0 - 45.0) * 110_540.0;
        assert!((xy[1].0 - expected_x).abs() < 1e-6, "{}", xy[1].0);
        assert!((xy[1].1 - expected_y).abs() < 1e-6, "{}", xy[1].1);
        let forgot_cos = 111_320.0;
        let used_degrees = 111_320.0 * 45.0_f64.cos();
        assert!((expected_x - forgot_cos).abs() > 1_000.0);
        assert!((expected_x - used_degrees).abs() > 1_000.0);
    }

    #[test]
    fn haversine_equator_one_degree() {
        let d = haversine_m(0.0, 0.0, 0.0, 1.0);
        let expected = 6_371_000.0 * std::f64::consts::PI / 180.0;
        assert!((d - expected).abs() < 1e-4, "{d} vs {expected}");
        assert_eq!(format!("{d:.1}"), "111194.9");
    }

    #[test]
    fn distance_sums_every_segment() {
        let points = [
            Point {
                lat: 0.0,
                lon: 0.0,
                ele: 0.0,
            },
            Point {
                lat: 0.0,
                lon: 1.0,
                ele: 0.0,
            },
            Point {
                lat: 1.0,
                lon: 1.0,
                ele: 0.0,
            },
        ];
        let sum = haversine_m(0.0, 0.0, 0.0, 1.0) + haversine_m(0.0, 1.0, 1.0, 1.0);
        let direct = haversine_m(0.0, 0.0, 1.0, 1.0);
        assert!((sum - direct).abs() > 1_000.0);
        assert!((total_distance(&points) - sum).abs() < 1e-6);
    }

    #[test]
    fn douglas_peucker_recurses() {
        let lat0 = 0.0;
        let lon0 = 0.0;
        let at = |east: f64, north: f64, ele: f64| Point {
            lat: lat0 + north / 110_540.0,
            lon: lon0 + east / 111_320.0,
            ele,
        };
        let points = [
            at(0.0, 0.0, 1.0),
            at(10.0, 20.0, 2.0),
            at(20.0, 0.0, 3.0),
            at(30.0, 0.0, 4.0),
        ];
        let xy = project(&points);
        let kept = |epsilon: f64| douglas_peucker(&points, &xy, epsilon);
        assert_eq!(kept(25.0).len(), 2);
        assert_eq!(kept(10.0).len(), 3);
        assert_eq!(kept(1.0).len(), 4);
        assert_eq!(kept(10.0)[1].ele, 2.0);
    }

    #[test]
    fn middle_point_is_five_meters_off_the_line() {
        let pts = parse_gpx(&line_gpx());
        assert_eq!(pts.len(), 3);
        let xy = project(&pts);
        let dist = perpendicular_distance(xy[1], xy[0], xy[2]);
        assert!((dist - 5.0).abs() < 1e-6, "distance {dist}");
        assert!(xy[0].0.abs() < 1e-9 && xy[0].1.abs() < 1e-9);
        assert!((xy[1].0 - 100.0).abs() < 1e-6, "mid x {}", xy[1].0);
        assert!((xy[1].1 - 5.0).abs() < 1e-6, "mid y {}", xy[1].1);
        assert!((xy[2].0 - 200.0).abs() < 1e-6, "end x {}", xy[2].0);
        assert!(xy[2].1.abs() < 1e-6, "end y {}", xy[2].1);
    }

    #[test]
    fn epsilon_10_keeps_two_epsilon_1_keeps_three() {
        let gpx = line_gpx();
        let (out10, _, path10) = invoke("line-eps-10", &gpx, "10");
        let (out1, _, path1) = invoke("line-eps-1", &gpx, "1");
        let out10 = out10.unwrap();
        let out1 = out1.unwrap();
        let pts = parse_gpx(&gpx);
        let expected = format!(
            "distance_m {:.1}\nelevation_gain_m 70.0\n",
            total_distance(&pts)
        );
        assert_eq!(out10, expected);
        assert_eq!(out1, expected);
        assert_eq!(out10.lines().count(), 2);

        let text10 = fs::read_to_string(&path10).unwrap();
        let text1 = fs::read_to_string(&path1).unwrap();
        assert_eq!(text10.matches("<trkpt ").count(), 2);
        assert_eq!(text1.matches("<trkpt ").count(), 3);
        assert!(!text10.contains("<ele>80.0</ele>"));
        assert!(text1.contains("<ele>80.0</ele>"));
        assert!(text10.contains("<ele>10.0</ele>"));
        assert!(text10.contains("<ele>15.0</ele>"));
        assert!(!text10.contains("ignore-me"));
        assert!(!text10.contains("<time"));
        assert_precision(&text10);
        assert_precision(&text1);

        let kept = parse_gpx(&text10);
        assert_eq!(kept.len(), 2);
        assert!((kept[0].lat - pts[0].lat).abs() < 1e-6);
        assert!((kept[0].lon - pts[0].lon).abs() < 1e-6);
        assert!((kept[1].lat - pts[2].lat).abs() < 1e-6);
        assert!((kept[1].lon - pts[2].lon).abs() < 1e-6);
    }

    #[test]
    fn known_elevation_gain() {
        let (stdout, _, output) = invoke("elevation", ELEVATION_GPX, "10");
        assert_eq!(
            stdout.unwrap(),
            "distance_m 3335.8\nelevation_gain_m 65.5\n"
        );
        let text = fs::read_to_string(&output).unwrap();
        assert_eq!(text.matches("<trkpt ").count(), 2);
        assert!(text.contains("<ele>100.0</ele>"));
        assert!(text.contains("<ele>155.5</ele>"));
        assert!(!text.contains("<ele>140.0</ele>"));
        assert!(!text.contains("<ele>130.0</ele>"));
        for marker in [
            "ignore-me",
            "9999",
            "5000",
            "8000",
            "1000",
            "nope",
            "<time",
            "<name",
        ] {
            assert!(!text.contains(marker), "output kept {marker}");
        }
        assert_precision(&text);
        let kept = parse_gpx(&text);
        assert_eq!(kept.len(), 2);
        assert!((kept[0].ele - 100.0).abs() < 1e-9);
        assert!((kept[1].ele - 155.5).abs() < 1e-9);
    }

    #[test]
    fn fewer_than_two_points_exits_2() {
        let gpx = r#"<gpx>
          <wpt lat="1" lon="2"><ele>3</ele></wpt>
          <trk><trkseg>
            <trkpt lat="1" lon="2"><ele>3</ele></trkpt>
            <trkpt lat="4" lon="5"></trkpt>
          </trkseg></trk>
        </gpx>"#;
        let (err, _, output) = invoke("few", gpx, "10");
        let err = err.unwrap_err();
        assert_eq!(err.code, 2);
        assert!(!output.exists());

        let (empty, _, _) = invoke("empty", "", "10");
        assert_eq!(empty.unwrap_err().code, 2);
    }

    #[test]
    fn bad_args_exit_1() {
        let err = run(&[]).unwrap_err();
        assert_eq!(err.code, 1);
        let err = run(&[
            "--input".into(),
            "in.gpx".into(),
            "--epsilon".into(),
            "nope".into(),
            "--output".into(),
            "out.gpx".into(),
        ])
        .unwrap_err();
        assert_eq!(err.code, 1);
        let missing = workspace_tmp("missing").join("no-such.gpx");
        let output = workspace_tmp("missing").join("out.gpx");
        let err = run(&[
            "--input".into(),
            missing.to_str().unwrap().into(),
            "--epsilon".into(),
            "10".into(),
            "--output".into(),
            output.to_str().unwrap().into(),
        ])
        .unwrap_err();
        assert_eq!(err.code, 1);
    }
}
