use cgmath::{Point2, Point3, Vector2};
use clap::value_t;
use collision::{Aabb, Aabb2, Aabb3};
use point_cloud_client::PointCloudClient;
use point_viewer::color::{Color, TRANSPARENT, WHITE};
use point_viewer::octree::OctreeFactory;
use point_viewer_grpc::octree_from_grpc_address;
use std::error::Error;
use std::path::Path;
use xray::generation::{
    xray_from_points, ColoringStrategyArgument, ColoringStrategyKind, TileBackgroundColorArgument,
};

fn parse_arguments() -> clap::ArgMatches<'static> {
    // TODO(sirver): pull out a function for common args.
    clap::App::new("build_xray_tile")
        .version("1.0")
        .author("Holger H. Rapp <hrapp@lyft.com>")
        .args(&[
            clap::Arg::with_name("output_filename")
                .help("Output filename to write into.")
                .default_value("output.png")
                .long("output_filename")
                .takes_value(true),
            clap::Arg::with_name("resolution")
                .help("Size of 1px in meters.")
                .long("resolution")
                .default_value("0.05"),
            clap::Arg::with_name("coloring_strategy")
                .long("coloring_strategy")
                .takes_value(true)
                .possible_values(&ColoringStrategyArgument::variants())
                .default_value("xray"),
            clap::Arg::with_name("min_intensity")
                .help(
                    "Minimum intensity of all points for color scaling. \
                     Only used for 'colored_with_intensity'.",
                )
                .long("min_intensity")
                .takes_value(true)
                .required_if("coloring_strategy", "colored_with_intensity"),
            clap::Arg::with_name("max_intensity")
                .help(
                    "Minimum intensity of all points for color scaling. \
                     Only used for 'colored_with_intensity'.",
                )
                .long("max_intensity")
                .takes_value(true)
                .required_if("coloring_strategy", "colored_with_intensity"),
            clap::Arg::with_name("max_stddev")
                .help(
                    "Maximum stddev for colored_with_height_stddev. Every stddev above this \
                     will be clamped to this value and appear saturated in the X-Rays. \
                     Only used for 'colored_with_height_stddev'.",
                )
                .long("max_stddev")
                .takes_value(true)
                .required_if("coloring_strategy", "colored_with_height_stddev"),
            clap::Arg::with_name("octree_locations")
                .help("Octree locations to turn into xrays.")
                .index(1)
                .multiple(true)
                .required(true),
            clap::Arg::with_name("min_x")
                .long("min_x")
                .takes_value(true)
                .help("Bounding box minimum x in meters.")
                .required(true),
            clap::Arg::with_name("min_y")
                .long("min_y")
                .takes_value(true)
                .help("Bounding box minimum y in meters.")
                .required(true),
            clap::Arg::with_name("max_x")
                .long("max_x")
                .takes_value(true)
                .help("Bounding box maximum x in meters.")
                .required(true),
            clap::Arg::with_name("max_y")
                .long("max_y")
                .takes_value(true)
                .help("Bounding box maximum y in meters.")
                .required(true),
            clap::Arg::with_name("tile_background_color")
                .long("tile_background_color")
                .takes_value(true)
                .possible_values(&TileBackgroundColorArgument::variants())
                .default_value("white"),
        ])
        .get_matches()
}

fn run(
    octree_locations: &[String],
    output_filename: &Path,
    resolution: f64,
    coloring_strategy_kind: &ColoringStrategyKind,
    tile_background_color: Color<u8>,
    bbox2: &Aabb2<f64>,
) -> Result<(), Box<Error>> {
    let octree_factory = OctreeFactory::new().register("grpc://", octree_from_grpc_address);
    let point_cloud_client = PointCloudClient::new(octree_locations, octree_factory)?;
    let bbox3 = point_cloud_client.bounding_box();
    let bbox3 = Aabb3::new(
        Point3::new(
            bbox2.min().x.max(bbox3.min().x),
            bbox2.min().y.max(bbox3.min().y),
            bbox3.min().z,
        ),
        Point3::new(
            bbox2.max().x.min(bbox3.max().x),
            bbox2.max().y.min(bbox3.max().y),
            bbox3.max().z,
        ),
    );
    let image_width = (bbox2.dim().x / resolution).ceil() as u32;
    let image_height = (bbox2.dim().y / resolution).ceil() as u32;
    if !xray_from_points(
        &point_cloud_client,
        &None,
        &bbox3,
        output_filename,
        Vector2::new(image_width, image_height),
        coloring_strategy_kind.new_strategy(),
        tile_background_color,
    ) {
        println!("No points in bounding box. No output written.");
    }
    Ok(())
}

pub fn main() {
    let matches = parse_arguments();
    let resolution = value_t!(matches, "resolution", f64).expect("resolution could not be parsed.");
    let coloring_strategy_kind = {
        use crate::ColoringStrategyArgument::*;
        let arg = value_t!(matches, "coloring_strategy", ColoringStrategyArgument)
            .expect("coloring_strategy is invalid");
        match arg {
            xray => ColoringStrategyKind::XRay,
            colored => ColoringStrategyKind::Colored,
            colored_with_intensity => ColoringStrategyKind::ColoredWithIntensity(
                value_t!(matches, "min_intensity", f32).unwrap_or(1.),
                value_t!(matches, "max_intensity", f32).unwrap_or(1.),
            ),
            colored_with_height_stddev => ColoringStrategyKind::ColoredWithHeightStddev(
                value_t!(matches, "max_stddev", f32).unwrap_or(1.),
            ),
        }
    };
    let tile_background_color = {
        let arg = value_t!(
            matches,
            "tile_background_color",
            TileBackgroundColorArgument
        )
        .expect("tile_background_color is invalid");
        match arg {
            TileBackgroundColorArgument::white => WHITE.to_u8(),
            TileBackgroundColorArgument::transparent => TRANSPARENT.to_u8(),
        }
    };
    let octree_locations = matches
        .values_of("octree_locations")
        .unwrap()
        .map(String::from)
        .collect::<Vec<_>>();
    let output_filename = Path::new(matches.value_of("output_filename").unwrap());
    let min_x = value_t!(matches, "min_x", f64).expect("min_x could not be parsed.");
    let min_y = value_t!(matches, "min_y", f64).expect("min_y could not be parsed.");
    let max_x = value_t!(matches, "max_x", f64).expect("max_x could not be parsed.");
    let max_y = value_t!(matches, "max_y", f64).expect("max_y could not be parsed.");

    let bbox2 = Aabb2::new(Point2::new(min_x, min_y), Point2::new(max_x, max_y));
    run(
        &octree_locations,
        output_filename,
        resolution,
        &coloring_strategy_kind,
        tile_background_color,
        &bbox2,
    )
    .unwrap();
}
