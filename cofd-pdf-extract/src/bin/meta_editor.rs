use std::{
	collections::HashMap,
	fs::{self, File},
	ops::{Range, RangeBounds},
	path::{Path, PathBuf},
};

use eframe::{
	egui::{self, FontSelection, TextEdit, TextFormat},
	epaint::{self, text::cursor::Cursor, Color32, FontId},
};

use cofd_pdf_extract::{
	hash,
	meta::{Op, SectionDefinition, SourceMeta, Span},
	source_file::{extract_pages, make_section},
};
use env_logger::fmt::Color;
use serde::Serialize;
use serde_json::ser::PrettyFormatter;

fn main() -> eframe::Result<()> {
	let native_options = eframe::NativeOptions::default();
	eframe::run_native(
		"My egui App",
		native_options,
		Box::new(|cc| Box::new(MetaEditorApp::new(cc))),
	)
}

struct MetaEditorApp {
	meta: SourceMeta,
	meta_path: PathBuf,
	pages: HashMap<usize, String>,
	path: PathBuf,

	selected_section: Option<usize>,
	show_full_text: bool,
	last_range: Option<Range<usize>>,
	pages_start: String,
	pages_end: String,
}

impl MetaEditorApp {
	fn new(cc: &eframe::CreationContext<'_>) -> Self {
		let args: Vec<_> = std::env::args().collect();
		let path = PathBuf::from(args.get(1).unwrap());

		let hash = hash::hash(&path).unwrap();

		let (meta, meta_path) = fs::read_dir("meta")
			.unwrap()
			.into_iter()
			.filter_map(|entry| entry.ok().map(|e| e.path()))
			.filter(|path| {
				path.extension()
					.and_then(|ext| Some(ext.eq("json")))
					.unwrap_or(false)
			})
			.map(|path| -> anyhow::Result<(SourceMeta, PathBuf)> {
				Ok((serde_json::de::from_reader(File::open(&path)?)?, path))
			})
			.filter_map(|r| r.ok())
			.find(|(meta, path)| meta.hash.eq(&hash))
			.unwrap_or_else(|| {
				(
					SourceMeta {
						hash,
						sections: Vec::new(),
						timestamp: 0,
					},
					Path::new("meta")
						.join(path.file_name().unwrap())
						.with_extension("json"),
				)
			});

		let pages = extract_pages(&path).unwrap();

		Self {
			meta,
			meta_path,
			pages,
			path,
			selected_section: None,
			show_full_text: true,
			last_range: None,
			pages_end: "".to_string(),
			pages_start: "".to_string(),
		}
	}

	fn highlight(
		show_full_text: bool,
		ui: &egui::Ui,
		text: &str,
		font_id: FontId,
		wrap_width: f32,
		section: &SectionDefinition,
	) -> epaint::text::LayoutJob {
		let mut layout_job =
			epaint::text::LayoutJob::simple(text.to_string(), font_id, Color32::GRAY, wrap_width);

		if show_full_text {
			if let Some(range) = &section.range {
				layout_job.sections.clear();

				let byte_range = match range {
					cofd_pdf_extract::meta::Span::Range(range) => 0..range.start,
					cofd_pdf_extract::meta::Span::From(from) => 0..from.start,
				};

				layout_job.sections.push(epaint::text::LayoutSection {
					leading_space: 0.0,
					byte_range,
					format: TextFormat::default(),
				});

				let byte_range = match range {
					cofd_pdf_extract::meta::Span::Range(range) => range.clone(),
					cofd_pdf_extract::meta::Span::From(from) => from.start..text.len(),
				};
				let format = TextFormat {
					color: Color32::BLACK,
					background: Color32::GRAY,
					..Default::default()
				};

				layout_job.sections.push(epaint::text::LayoutSection {
					leading_space: 0.0,
					byte_range,
					format,
				});

				match range {
					cofd_pdf_extract::meta::Span::Range(range) => {
						let byte_range = range.end..text.len();

						layout_job.sections.push(epaint::text::LayoutSection {
							leading_space: 0.0,
							byte_range,
							format: TextFormat::default(),
						});
					}
					_ => {}
				}
			}
		}

		layout_job
	}
}

impl eframe::App for MetaEditorApp {
	fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
		egui::SidePanel::left("sidebar")
			.resizable(false)
			.show(ctx, |ui| {
				egui_extras::TableBuilder::new(ui)
					.column(egui_extras::Column::remainder())
					.body(|mut body| {
						for (i, section) in self.meta.sections.iter().enumerate() {
							body.row(18.0, |mut row| {
								row.col(|ui| {
									if ui
										.selectable_value(
											&mut self.selected_section,
											Some(i),
											&section.name,
										)
										.clicked()
									{
										let selection = self
											.meta
											.sections
											.get(self.selected_section.unwrap())
											.unwrap();
										self.pages_start = selection.pages.start().to_string();
										self.pages_end = selection.pages.end().to_string();
									}
								});
							})
						}
					});

				if let Some(selected_section) = self.selected_section {
					if let Some(section) = self.meta.sections.get_mut(selected_section) {
						ui.text_edit_singleline(&mut section.name);

						// ui.push_id("XDDD", |ui| {
						ui.horizontal_top(|ui| {
							if ui
								.add(
									TextEdit::singleline(&mut self.pages_start)
										.id_source("pages_start"),
								)
								.changed()
							{
								section.pages =
									(self.pages_start.parse().unwrap())..=*section.pages.end();
							}
							if ui
								.add(
									TextEdit::singleline(&mut self.pages_end)
										.id_source("pages_end"),
								)
								.changed()
							{
								section.pages =
									*section.pages.start()..=(self.pages_end.parse().unwrap())
							}
						});
						// });
					}
				}

				if ui.button("Add section").clicked() {
					self.meta.sections.push(SectionDefinition {
						name: String::from("Unnamed"),
						pages: 1..=2,
						range: None,
						kind: cofd_pdf_extract::page_kind::PageKind::Merit(None),
						ops: Vec::new(),
					})
				}

				if ui.button("Save").clicked() {
					let mut ser = serde_json::Serializer::with_formatter(
						File::create(&self.meta_path).unwrap(),
						PrettyFormatter::with_indent(b"\t"),
					);
					self.meta.serialize(&mut ser);
				}

				ui.checkbox(&mut self.show_full_text, "Show full text");
			});

		egui::CentralPanel::default().show(ctx, |ui| {
			egui::ScrollArea::vertical()
				// .id_source("source")
				.show(ui, |ui| {
					if let Some(selected_section) = self.selected_section {
						// let mut text: &str = self.pages.get(&2).unwrap().as_str();
						let section_def = self.meta.sections.get_mut(selected_section).unwrap();
						// let sec = section_def.clone();

						let font_id = FontSelection::Default.resolve(ui.style());
						let mut layouter = |ui: &egui::Ui, text: &str, wrap_width: f32| {
							let mut layout_job = MetaEditorApp::highlight(
								self.show_full_text,
								ui,
								text,
								font_id.clone(),
								wrap_width,
								&section_def,
							);
							layout_job.wrap.max_width = wrap_width;
							ui.fonts(|f| f.layout_job(layout_job))
						};
						let section = make_section(&self.pages, section_def, self.show_full_text);

						let mut text: &str = section.extract.as_str();
						use egui::TextBuffer as _;

						let output = egui::TextEdit::multiline(&mut text)
							.layouter(&mut layouter)
							.desired_width(f32::INFINITY)
							.show(ui);

						if let Some(cursor_range) = output.cursor_range {
							if !cursor_range.is_empty() {
								let [start, end] = cursor_range.sorted_cursors();
								let start = text.byte_index_from_char_index(start.ccursor.index);
								let end = text.byte_index_from_char_index(end.ccursor.index);

								self.last_range = Some(start..end);
							}
						}

						output.response.context_menu(|ui| {
							if ui.button("Set range").clicked() {
								if let Some(range) = &self.last_range {
									section_def.range = Some(Span::Range(range.clone()));
								}

								ui.close_menu();
							}

							if ui.button("Delete").clicked() {
								if let Some(range) = &self.last_range {
									let range = range.start..=(range.end - 1);

									section_def.ops.push(Op::Delete { range })
								}

								ui.close_menu();
							}
						});
					}
				});
		});
	}
}
