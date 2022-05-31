const URL_CASTANET: &'static str = "https://classifieds.castanet.net";
const URL_DISCORD_WEBHOOK: &'static str = "https://discord.com/api/webhooks/<INSERT URL>";
const LIMIT_PER_PAGE: usize = 100;
const LIMIT_COST: f32 = 20_000.00;
const LIMIT_MILEAGE: f32 = 160_000.00;

use reqwest::blocking::Client;
use rusqlite::{params, Connection};
use tl::Node;

#[derive(Debug)]
struct Vehicle {
	model: String,
	category: String,
	price: String,
	description: String,
	city: String,
	url: String,
	image: String,

	year: Option<String>,
	make: Option<String>,
	mileage: Option<String>,
	ad_type: Option<String>,
}

impl Vehicle {
	fn new() -> Self {
		Vehicle {
			model: String::new(),
			category: String::new(),
			price: String::new(),
			description: String::new(),
			city: String::new(),
			url: String::new(),
			image: String::new(),

			year: None,
			make: None,
			mileage: None,
			ad_type: None,
		}
	}
}

fn post_discord_message(client: &Client, vehicle: Vehicle) {
	let params = [(
		"content",
		format!(
			"{} {} {}: {}, {} km. {} {}{}",
			vehicle.year.as_ref().unwrap(),
			vehicle.make.as_ref().unwrap(),
			vehicle.model,
			vehicle.price,
			vehicle.mileage.as_ref().unwrap(),
			vehicle.image,
			URL_CASTANET,
			vehicle.url,
		),
	)];
	client
		.post(URL_DISCORD_WEBHOOK)
		.form(&params)
		.send()
		.unwrap();
}

fn get_attribute(tag: &tl::HTMLTag, attr: &str) -> String {
	tag.attributes()
		.get(attr)
		.unwrap()
		.unwrap()
		.as_utf8_str()
		.to_string()
}

fn get_classifieds(page: isize) -> Vec<Vehicle> {
	let mut classifieds = Vec::new();

	let body = reqwest::blocking::get(format!(
		"{}/cat/vehicles/?perpage={}&p={}",
		URL_CASTANET, LIMIT_PER_PAGE, page
	))
	.unwrap()
	.text()
	.unwrap();
	let dom = tl::parse(&body, tl::ParserOptions::default()).unwrap();
	let parser = dom.parser();

	let column = dom
		.get_element_by_id("left_column")
		.unwrap()
		.get(parser)
		.unwrap()
		.as_tag()
		.unwrap();

	let entries = column.query_selector(parser, ".prod_container").unwrap();

	for entry in entries {
		let mut vehicle = Vehicle::new();
		let mut count = 0;

		vehicle.url = get_attribute(entry.get(parser).unwrap().as_tag().unwrap(), "href");

		for i in entry.get(parser).unwrap().children().unwrap().all(parser) {
			match i {
				Node::Tag(tag) => {
					if tag.name() == "h2" {
						vehicle.model = tag.inner_text(parser).to_string();
					} else if tag.name() == "span" {
						if count == 0 {
							vehicle.description = tag.inner_text(parser).to_string();
						} else if count == 1 {
							vehicle.city = tag.inner_text(parser).to_string();
						}
						count += 1;
					} else if tag.name() == "div" {
						if tag.attributes().is_class_member("price") {
							vehicle.price = tag.inner_text(parser).trim().to_string();
						} else if tag.attributes().is_class_member("cat_path") {
							vehicle.category = tag.inner_text(parser).to_string();
						}
					} else if tag.name() == "img" {
						if vehicle.image == "" {
							vehicle.image = get_attribute(tag, "src");
						}
					}
				}
				_ => {}
			}
		}

		classifieds.push(vehicle);
	}

	classifieds
}

fn get_listing(vehicle: &mut Vehicle) {
	//let body = fs::read_to_string("castanet2.html").unwrap();
	let body = reqwest::blocking::get(format!("{}{}", URL_CASTANET, vehicle.url))
		.unwrap()
		.text()
		.unwrap();
	let dom = tl::parse(&body, tl::ParserOptions::default()).unwrap();
	let parser = dom.parser();

	let mut details_div = dom.get_elements_by_class_name("prod_right");

	let mut count = 0;
	for entry in details_div
		.next()
		.unwrap()
		.get(parser)
		.unwrap()
		.children()
		.unwrap()
		.all(parser)
	{
		match entry {
			Node::Tag(tag) => {
				if tag.name() == "td" {
					let contents = tag.inner_text(parser).trim().to_string();
					match count {
						1 => vehicle.year = Some(contents),
						3 => vehicle.make = Some(contents),
						5 => vehicle.model = contents,
						7 => vehicle.mileage = Some(contents),
						15 => vehicle.ad_type = Some(contents),
						_ => {}
					}
					count += 1;
				}
			}
			_ => {}
		}
	}

	let mut description_div = dom.get_elements_by_class_name("description");

	let description_contents = description_div
		.next()
		.unwrap()
		.get(parser)
		.unwrap()
		.as_tag()
		.unwrap()
		.inner_text(parser);

	let mut splitter;
	if vehicle.ad_type == Some(String::from("Buisness")) {
		splitter = description_contents.splitn(2, "Send Message");
	} else {
		splitter = description_contents.splitn(2, "Email Seller");
	}
	let _ = splitter.next();
	let description = splitter.next();

	if description.is_some() {
		vehicle.description = description.unwrap().trim().to_string();
	}
}

fn main() {
	let client = Client::new();
	let conn = Connection::open("vehicles.db").unwrap();

	conn.execute(
		"
		CREATE TABLE IF NOT EXISTS castanet (
			url TEXT PRIMARY KEY,
			category TEXT NOT NULL,
			price TEXT NOT NULL,
			description TEXT NOT NULL,
			city TEXT NOT NULL,
			image TEXT NOT NULL,
			year TEXT,
			make TEXT,
			mileage TEXT,
			ad_type TEXT
		)",
		[],
	)
	.unwrap();

	let mut stmt_check = conn
		.prepare("SELECT * FROM castanet WHERE url = ?1")
		.unwrap();

	let mut stmt_insert = conn.prepare("INSERT INTO castanet (url, category, price, description, city, image, year, make, mileage, ad_type) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10) ").unwrap();

	let vehicles = get_classifieds(1);

	for mut entry in vehicles {
		let price_str = entry.price.replace("$", "").replace(",", "");
		let price = price_str.parse::<f32>();

		if !entry.category.contains("Trucks")
			&& !entry.category.contains("Vintage")
			&& price.is_ok()
			&& price.unwrap() < LIMIT_COST
		{
			let in_db = stmt_check.execute(params![&entry.url]);

			if in_db.is_ok() {
				get_listing(&mut entry);
				stmt_insert
					.execute(params![
						&entry.url,
						&entry.category,
						&entry.price,
						&entry.description,
						&entry.city,
						&entry.image,
						&entry.year,
						&entry.make,
						&entry.mileage,
						&entry.ad_type,
					])
					.unwrap();

				let mileage = entry.mileage.as_ref().unwrap().parse::<f32>();
				if mileage.is_ok() && mileage.unwrap() < LIMIT_MILEAGE {
					post_discord_message(&client, entry);
				}
			}
		}
	}
}
