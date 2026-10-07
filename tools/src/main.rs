// SPDX-License-Identifier: AGPL-3.0-or-later

//! Map tools. Only command so far:
//!
//! `tools convert --input <dem.tif> --out-dir <dir> [--cell 50]`
//!
//! Reads a single-band GeoTIFF of heights (metres) in a projected metre CRS and writes
//! `sim_height.bin` (bilinear, `cell` metres, f32 LE, row 0 = north), `terrain_r16.raw`
//! (source resolution, u16 LE, min..max -> 0..65535) and `map.toml` describing both.
//! Nodata / NaN samples are replaced by the minimum valid height.

use std::error::Error;
use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use tiff::decoder::{Decoder, DecodingResult};
use tiff::tags::Tag;

type Res<T> = Result<T, Box<dyn Error>>;

const USAGE: &str = "usage: tools convert --input <metric GeoTIFF> --out-dir <dir> [--cell <m>]";

/// GeoKey ids (GeoTIFF spec 1.0).
const GT_RASTER_TYPE_KEY: u16 = 1025;
const PROJECTED_CS_TYPE_KEY: u16 = 3072;
const RASTER_PIXEL_IS_POINT: u16 = 2;

/// Source DEM after decoding. `heights` is row-major, row 0 = north, nodata already filled.
#[derive(Debug)]
struct Dem {
    width: usize,
    height: usize,
    pixel_x: f64,
    pixel_y: f64,
    /// Projected coordinates of the centre of pixel (0, 0).
    origin_x: f64,
    origin_y: f64,
    epsg: Option<u16>,
    heights: Vec<f32>,
    min: f32,
    max: f32,
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Err(e) = run(&args) {
        eprintln!("error: {e}\n{USAGE}");
        std::process::exit(1);
    }
}

fn run(args: &[String]) -> Res<()> {
    let (cmd, rest) = args.split_first().ok_or("missing command")?;
    if cmd != "convert" {
        return Err(format!("unknown command '{cmd}'").into());
    }
    let mut input = None;
    let mut out_dir = None;
    let mut cell = 50.0;
    let mut it = rest.iter();
    while let Some(flag) = it.next() {
        let val = it
            .next()
            .ok_or_else(|| format!("missing value for {flag}"))?;
        match flag.as_str() {
            "--input" => input = Some(PathBuf::from(val)),
            "--out-dir" => out_dir = Some(PathBuf::from(val)),
            "--cell" => cell = val.parse()?,
            _ => return Err(format!("unknown flag '{flag}'").into()),
        }
    }
    let input = input.ok_or("--input is required")?;
    let out_dir = out_dir.ok_or("--out-dir is required")?;
    let dem = read_geotiff(&input)?;
    let (w, h) = convert(&dem, &out_dir, cell)?;
    println!(
        "source {}x{} @ {}x{} m, sim {w}x{h} @ {cell} m, height {}..{} m -> {}",
        dem.width,
        dem.height,
        dem.pixel_x,
        dem.pixel_y,
        dem.min,
        dem.max,
        out_dir.display()
    );
    Ok(())
}

fn read_geotiff(path: &Path) -> Res<Dem> {
    let mut dec = Decoder::new(BufReader::new(File::open(path)?))?;
    let (w, h) = dec.dimensions()?;
    let (width, height) = (w as usize, h as usize);

    let scale = dec.get_tag_f64_vec(Tag::ModelPixelScaleTag)?;
    let tie = dec.get_tag_f64_vec(Tag::ModelTiepointTag)?;
    let (pixel_x, pixel_y) = match scale.as_slice() {
        [x, y, ..] => (*x, *y),
        _ => return Err("ModelPixelScale needs at least 2 values".into()),
    };
    // Geographic (degree) rasters have sub-1 pixel sizes; the sim needs metres.
    if !(pixel_x >= 1.0 && pixel_y >= 1.0) {
        return Err(format!(
            "pixel size {pixel_x}x{pixel_y} does not look like metres; reproject with gdalwarp -t_srs EPSG:32636"
        )
        .into());
    }
    let [ti, tj, _, tx, ty, ..] = tie.as_slice() else {
        return Err("ModelTiepoint needs 6 values".into());
    };

    let keys = dec
        .find_tag_unsigned_vec::<u16>(Tag::GeoKeyDirectoryTag)?
        .unwrap_or_default();
    let geokey = |id: u16| {
        // Header is 4 shorts, then entries of (id, location, count, value); location 0 = inline.
        keys.get(4..)?
            .as_chunks::<4>()
            .0
            .iter()
            .find(|e| e[0] == id && e[1] == 0)
            .map(|e| e[3])
    };
    // PixelIsArea (default): tiepoint is the pixel's outer corner. PixelIsPoint: its centre.
    let half = if geokey(GT_RASTER_TYPE_KEY) == Some(RASTER_PIXEL_IS_POINT) {
        0.0
    } else {
        0.5
    };
    let origin_x = tx + (half - ti) * pixel_x;
    let origin_y = ty - (half - tj) * pixel_y;

    let nodata: Option<f64> = match dec.find_tag(Tag::GdalNodata)? {
        Some(v) => v
            .into_string()?
            .trim_matches(char::from(0))
            .trim()
            .parse()
            .ok(),
        None => None,
    };

    let mut heights: Vec<f32> = match dec.read_image()? {
        DecodingResult::F32(v) => v,
        DecodingResult::F64(v) => v.into_iter().map(|x| x as f32).collect(),
        DecodingResult::I16(v) => v.into_iter().map(f32::from).collect(),
        DecodingResult::U16(v) => v.into_iter().map(f32::from).collect(),
        _ => return Err("unsupported sample type (need f32, f64, i16 or u16)".into()),
    };
    if heights.len() != width * height {
        return Err("expected a single-band raster".into());
    }

    let is_valid = |x: f32| x.is_finite() && nodata.is_none_or(|nd| f64::from(x) != nd);
    let (mut min, mut max) = (f32::INFINITY, f32::NEG_INFINITY);
    for &x in heights.iter().filter(|&&x| is_valid(x)) {
        min = min.min(x);
        max = max.max(x);
    }
    if min > max {
        return Err("raster has no valid samples".into());
    }
    // ponytail: nodata -> min height; nearest-valid fill if real tiles ever have holes inland.
    for x in heights.iter_mut().filter(|x| !is_valid(**x)) {
        *x = min;
    }

    Ok(Dem {
        width,
        height,
        pixel_x,
        pixel_y,
        origin_x,
        origin_y,
        epsg: geokey(PROJECTED_CS_TYPE_KEY),
        heights,
        min,
        max,
    })
}

/// Bilinear sample at fractional pixel coordinates (clamped to the raster).
fn bilinear(dem: &Dem, fx: f64, fy: f64) -> f32 {
    let x0 = (fx.floor().max(0.0) as usize).min(dem.width - 1);
    let y0 = (fy.floor().max(0.0) as usize).min(dem.height - 1);
    let x1 = (x0 + 1).min(dem.width - 1);
    let y1 = (y0 + 1).min(dem.height - 1);
    let tx = (fx - x0 as f64).clamp(0.0, 1.0);
    let ty = (fy - y0 as f64).clamp(0.0, 1.0);
    let at = |x: usize, y: usize| f64::from(dem.heights[y * dem.width + x]);
    let top = at(x0, y0) * (1.0 - tx) + at(x1, y0) * tx;
    let bot = at(x0, y1) * (1.0 - tx) + at(x1, y1) * tx;
    (top * (1.0 - ty) + bot * ty) as f32
}

/// Writes the three output files; returns the sim grid size.
fn convert(dem: &Dem, out_dir: &Path, cell: f64) -> Res<(usize, usize)> {
    if !(cell.is_finite() && cell > 0.0) {
        return Err("--cell must be > 0".into());
    }
    // Sim samples stay inside the span of source pixel centres.
    let w = (((dem.width - 1) as f64 * dem.pixel_x) / cell).floor() as usize + 1;
    let h = (((dem.height - 1) as f64 * dem.pixel_y) / cell).floor() as usize + 1;

    let mut sim = Vec::with_capacity(w * h * 4);
    for j in 0..h {
        for i in 0..w {
            let v = bilinear(
                dem,
                i as f64 * cell / dem.pixel_x,
                j as f64 * cell / dem.pixel_y,
            );
            sim.extend_from_slice(&v.to_le_bytes());
        }
    }

    let range = f64::from(dem.max - dem.min);
    let mut r16 = Vec::with_capacity(dem.heights.len() * 2);
    for &x in &dem.heights {
        let n = if range > 0.0 {
            (f64::from(x - dem.min) / range * 65535.0).round() as u16
        } else {
            0
        };
        r16.extend_from_slice(&n.to_le_bytes());
    }

    let crs = match dem.epsg {
        Some(code) => format!("EPSG:{code}"),
        None => "unknown (projected, metres)".to_string(),
    };
    let toml = format!(
        "# Generated by `tools convert`. Sample (i, j) of sim_height.bin is at projected\n\
         # (origin_x + i * cell_size_m, origin_y - j * cell_size_m): row 0 = north, +j = south.\n\
         width = {w}\n\
         height = {h}\n\
         cell_size_m = {cell}\n\
         origin_x = {ox}\n\
         origin_y = {oy}\n\
         crs = \"{crs}\"\n\
         min_height_m = {min}\n\
         max_height_m = {max}\n\
         heights = \"sim_height.bin\" # f32 little-endian, row-major\n\
         \n\
         # Terrain3D import: 16-bit LE unsigned, source resolution, row 0 = north.\n\
         # height_m = min_height_m + value / 65535 * (max_height_m - min_height_m)\n\
         [terrain_r16]\n\
         file = \"terrain_r16.raw\"\n\
         width = {sw}\n\
         height = {sh}\n\
         pixel_size_x_m = {px}\n\
         pixel_size_y_m = {py}\n\
         min_height_m = {min}\n\
         max_height_m = {max}\n",
        ox = dem.origin_x,
        oy = dem.origin_y,
        min = dem.min,
        max = dem.max,
        sw = dem.width,
        sh = dem.height,
        px = dem.pixel_x,
        py = dem.pixel_y,
    );

    fs::create_dir_all(out_dir)?;
    fs::write(out_dir.join("sim_height.bin"), sim)?;
    fs::write(out_dir.join("terrain_r16.raw"), r16)?;
    fs::write(out_dir.join("map.toml"), toml)?;
    Ok((w, h))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use tiff::encoder::{colortype::Gray32Float, TiffEncoder};

    /// 5x4 raster, 100 m pixels, height = 10 * col + 1000 * row (north row = 0), one nodata.
    fn write_fixture(path: &Path) {
        let (w, h) = (5u32, 4u32);
        let mut data: Vec<f32> = (0..h)
            .flat_map(|r| (0..w).map(move |c| (10 * c + 1000 * r) as f32))
            .collect();
        data[(w * h - 1) as usize] = -9999.0; // nodata in the SE corner
        let mut enc = TiffEncoder::new(File::create(path).unwrap()).unwrap();
        let mut img = enc.new_image::<Gray32Float>(w, h).unwrap();
        let d = img.encoder();
        d.write_tag(Tag::ModelPixelScaleTag, &[100.0f64, 100.0, 0.0][..])
            .unwrap();
        d.write_tag(
            Tag::ModelTiepointTag,
            &[0.0f64, 0.0, 0.0, 500_000.0, 4_200_000.0, 0.0][..],
        )
        .unwrap();
        // GeoKeyDirectory: version 1.1.0, 2 keys: PixelIsArea, EPSG:32636.
        d.write_tag(
            Tag::GeoKeyDirectoryTag,
            &[1u16, 1, 0, 2, 1025, 0, 1, 1, 3072, 0, 1, 32636][..],
        )
        .unwrap();
        d.write_tag(Tag::GdalNodata, "-9999").unwrap();
        img.write_data(&data).unwrap();
    }

    fn read_f32(path: &Path) -> Vec<f32> {
        fs::read(path)
            .unwrap()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|b| f32::from_le_bytes(*b))
            .collect()
    }

    #[test]
    fn g8_convert_synthetic_geotiff() {
        let dir = std::env::temp_dir().join(format!("g8_test_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let tif = dir.join("dem.tif");
        write_fixture(&tif);
        let out = dir.join("out");
        let args: Vec<String> = [
            "convert",
            "--input",
            tif.to_str().unwrap(),
            "--out-dir",
            out.to_str().unwrap(),
            "--cell",
            "50",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        run(&args).unwrap();

        // Source spans 4x3 pixels of 100 m between centres -> 9x7 samples at 50 m.
        let sim = read_f32(&out.join("sim_height.bin"));
        assert_eq!(sim.len(), 9 * 7);
        assert_eq!(sim[0], 0.0); // NW pixel centre
        assert_eq!(sim[1], 5.0); // halfway between col 0 and 1
        assert_eq!(sim[9], 500.0); // halfway between row 0 and 1
        assert_eq!(sim[2 * 9 + 2], 1010.0); // exactly pixel (1, 1)
                                            // SE corner was nodata -> filled with min height (0).
        assert_eq!(sim[9 * 7 - 1], 0.0);

        let r16 = fs::read(out.join("terrain_r16.raw")).unwrap();
        assert_eq!(r16.len(), 5 * 4 * 2);
        let r = |k: usize| u16::from_le_bytes([r16[2 * k], r16[2 * k + 1]]);
        assert_eq!(r(0), 0);
        assert_eq!(r(18), 65535); // (col 3, row 3) = 3030 = max
        assert_eq!(r(19), 0); // nodata -> min

        let toml = fs::read_to_string(out.join("map.toml")).unwrap();
        for line in [
            "width = 9",
            "height = 7",
            "cell_size_m = 50",
            "origin_x = 500050",
            "origin_y = 4199950",
            "crs = \"EPSG:32636\"",
            "min_height_m = 0",
            "max_height_m = 3030",
            "pixel_size_x_m = 100",
        ] {
            assert!(toml.contains(line), "missing '{line}' in:\n{toml}");
        }
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn g8_rejects_geographic_degrees() {
        let dir = std::env::temp_dir().join(format!("g8_deg_{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let tif = dir.join("deg.tif");
        let mut enc = TiffEncoder::new(File::create(&tif).unwrap()).unwrap();
        let mut img = enc.new_image::<Gray32Float>(2, 2).unwrap();
        let d = img.encoder();
        d.write_tag(Tag::ModelPixelScaleTag, &[0.0003f64, 0.0003, 0.0][..])
            .unwrap();
        d.write_tag(
            Tag::ModelTiepointTag,
            &[0.0f64, 0.0, 0.0, 34.0, 38.0, 0.0][..],
        )
        .unwrap();
        img.write_data(&[1.0f32, 2.0, 3.0, 4.0]).unwrap();
        assert!(read_geotiff(&tif).is_err());
        fs::remove_dir_all(&dir).unwrap();
    }
}
