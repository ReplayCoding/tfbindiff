use eframe::egui;
use egui::RichText;
use egui_extras::TableBuilder;

use tfbindiff::{
    compare::FunctionChange, instruction_wrapper::InstructionWrapper, program::Program,
    util::ProgramInstructionFormatter,
};

use crate::split_diff::DiffCell;

struct CachedFunctionChange {
    name: String,
    mangled_name: String,
    address1: u64,
    address2: u64,

    lines: Vec<(DiffCell<String>, DiffCell<String>)>,
}

impl CachedFunctionChange {
    fn new(
        program1: &'static Program,
        program2: &'static Program,
        change: &FunctionChange,
        name: &str,
    ) -> Self {
        Self {
            name: name.to_string(),
            mangled_name: change.name().to_string(),
            address1: change.address1(),
            address2: change.address2(),
            lines: Self::build_split_diff_lines(program1, program2, change),
        }
    }

    fn build_split_diff_lines(
        program1: &'static Program,
        program2: &'static Program,
        change: &FunctionChange,
    ) -> Vec<(DiffCell<String>, DiffCell<String>)> {
        let (instructions1, instructions2) = change.instructions();
        // NOTE: Lcs panics on oob, wtf?
        let diff_ops =
            similar::capture_diff_slices(similar::Algorithm::Myers, instructions1, instructions2);

        let split_diff = crate::split_diff::build(instructions1, instructions2, &diff_ops);

        let mut formatter1 = ProgramInstructionFormatter::new(program1);
        let mut formatter2 = ProgramInstructionFormatter::new(program2);

        let fmt_line =
            |formatter: &mut ProgramInstructionFormatter, instr: &InstructionWrapper| -> String {
                format!("{:08x}\t{}", instr.get().ip(), formatter.format(instr))
            };

        let fmt_cell = |formatter: &mut ProgramInstructionFormatter,
                        cell: &DiffCell<InstructionWrapper>| {
            match cell {
                DiffCell::Hidden => DiffCell::Hidden,
                DiffCell::Collapsed => DiffCell::Collapsed,
                DiffCell::Default(i) => DiffCell::Default(fmt_line(formatter, i)),
                DiffCell::Insert(i) => DiffCell::Insert(fmt_line(formatter, i)),
                DiffCell::Delete(i) => DiffCell::Delete(fmt_line(formatter, i)),
            }
        };

        let formatted_lines: Vec<_> = split_diff
            .iter()
            .map(|(a, b)| (fmt_cell(&mut formatter1, a), fmt_cell(&mut formatter2, b)))
            .collect();

        formatted_lines
    }
}

enum DiffViewerMode {
    FunctionList,
    Diff,
}

struct DiffViewerApp {
    program1: &'static Program,
    program2: &'static Program,

    changes: Vec<(String, FunctionChange)>,
    current_cached_change: Option<CachedFunctionChange>,
    mode: DiffViewerMode,
}

impl DiffViewerApp {
    fn new(
        _cc: &eframe::CreationContext<'_>,
        program1: &'static Program,
        program2: &'static Program,
        changes: Vec<FunctionChange>,
    ) -> Self {
        // Customize egui here with cc.egui_ctx.set_fonts and cc.egui_ctx.set_visuals.
        // Restore app state using cc.storage (requires the "persistence" feature).
        // Use the cc.gl (a glow::Context) to create graphics shaders and buffers that you can use
        // for e.g. egui::PaintCallback.
        Self {
            program1,
            program2,

            changes: changes
                .into_iter()
                .map(|change| {
                    (
                        tfbindiff::util::demangle_symbol(change.name())
                            .unwrap_or_else(|| change.name().to_string()),
                        change,
                    )
                })
                .collect(),
            current_cached_change: None,
            mode: DiffViewerMode::FunctionList,
        }
    }

    fn draw_function_list(&mut self, ui: &mut egui::Ui) {
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), |ui| {
            ui.heading("Functions");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                ui.heading(format!("{} changes found", self.changes.len()));
            });
        });
        ui.separator();

        egui::ScrollArea::vertical()
            .auto_shrink([false, true])
            .show_rows(
                ui,
                ui.text_style_height(&egui::TextStyle::Button),
                self.changes.len(),
                |ui, range| {
                    for idx in range {
                        let (name, change) = &self.changes[idx];

                        ui.with_layout(egui::Layout::top_down_justified(egui::Align::Min), |ui| {
                            let button = ui.add(egui::Button::new(name).frame(false));
                            if button.clicked() {
                                self.current_cached_change = Some(CachedFunctionChange::new(
                                    self.program1,
                                    self.program2,
                                    change,
                                    name,
                                ));
                                self.mode = DiffViewerMode::Diff;
                            }
                        });
                    }
                },
            );
    }

    fn draw_diff_view(&mut self, ui: &mut egui::Ui) {
        let change = self
            .current_cached_change
            .as_ref()
            .expect("current cached change should never be None here");

        ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), |ui| {
            let back_button = ui.button("Back");
            if back_button.clicked() {
                self.mode = DiffViewerMode::FunctionList;
            }

            ui.heading(format!("Comparing {}", &change.name))
                .on_hover_text(&change.mangled_name);
            // TODO: Make the addresses copyable
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                ui.heading(format!(
                    "{:08x} vs {:08x}",
                    change.address1, change.address2
                ));
            })
        });
        ui.separator();

        ui.scope(|ui| {
            let text_style = egui::TextStyle::Monospace;
            let text_height = ui.text_style_height(&text_style);
            ui.style_mut().override_text_style = Some(text_style);

            let column_width = ui.available_width() / 2.0;
            let available_height = ui.available_height();

            let id = ui.id().with(change.address1);
            ui.push_id(id, |ui| {
                TableBuilder::new(ui)
                    .striped(false)
                    .cell_layout(egui::Layout::left_to_right(egui::Align::Min))
                    .resizable(false)
                    .auto_shrink([false, false])
                    .columns(egui_extras::Column::exact(column_width), 2)
                    .min_scrolled_height(available_height)
                    .body(|body| {
                        body.rows(text_height, change.lines.len(), |mut row| {
                            let (line1, line2) = &change.lines[row.index()];
                            let build_line = |line: &DiffCell<String>| match line {
                                DiffCell::Hidden => RichText::new(""),
                                DiffCell::Collapsed => RichText::new("..."),

                                DiffCell::Default(line) => RichText::new(line),
                                DiffCell::Insert(line) => {
                                    RichText::new(line).color(egui::Color32::GREEN)
                                }
                                DiffCell::Delete(line) => {
                                    RichText::new(line).color(egui::Color32::RED)
                                }
                            };

                            row.col(|ui| {
                                ui.label(build_line(line1));
                            });
                            row.col(|ui| {
                                ui.label(build_line(line2));
                            });
                        });
                    });
            })
        });
    }
}

impl eframe::App for DiffViewerApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| match self.mode {
            DiffViewerMode::FunctionList => self.draw_function_list(ui),
            DiffViewerMode::Diff => self.draw_diff_view(ui),
        });
    }
}

pub fn run(program1: &'static Program, program2: &'static Program, changes: Vec<FunctionChange>) {
    eframe::run_native(
        "tfbindiff viewer",
        eframe::NativeOptions::default(),
        Box::new(move |cc| Box::new(DiffViewerApp::new(cc, program1, program2, changes))),
    )
    .unwrap();
}
