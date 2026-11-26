//! Interface texts in the Windows display language.
//!
//! Calendar and priority names exist for English, Spanish, Portuguese, French and German
//! (as in v1.2). The rest of the interface is Spanish (matching the Omarchy panel) or
//! English for every other language.

use chrono::NaiveDate;

use crate::dates::{day_diff, weekday_index};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Language {
    En,
    Es,
    Pt,
    Fr,
    De,
}

pub const LANGUAGES: [Language; 5] = [
    Language::En,
    Language::Es,
    Language::Pt,
    Language::Fr,
    Language::De,
];

impl Language {
    pub fn code(self) -> &'static str {
        match self {
            Language::En => "en",
            Language::Es => "es",
            Language::Pt => "pt",
            Language::Fr => "fr",
            Language::De => "de",
        }
    }

    /// `es_CO`, `pt-BR`, `de_DE.UTF-8` -> a supported language, else English.
    pub fn from_tag(tag: &str) -> Language {
        let code = tag.replace('-', "_");
        let code = code.split(['_', '.']).next().unwrap_or("").to_lowercase();
        LANGUAGES
            .into_iter()
            .find(|l| l.code() == code)
            .unwrap_or(Language::En)
    }

    /// The primary language of a Windows LANGID (`GetUserDefaultUILanguage`).
    pub fn from_langid(langid: u16) -> Language {
        match langid & 0x3ff {
            0x0a => Language::Es,
            0x16 => Language::Pt,
            0x0c => Language::Fr,
            0x07 => Language::De,
            _ => Language::En,
        }
    }

    pub fn texts(self) -> &'static Texts {
        match self {
            Language::Es => &ES,
            Language::Pt => &PT,
            Language::Fr => &FR,
            Language::De => &DE,
            Language::En => &EN,
        }
    }
}

/// Every string the interface shows.
#[derive(Debug)]
pub struct Texts {
    pub months: [&'static str; 12],
    pub months_short: [&'static str; 12],
    /// Monday first.
    pub weekdays: [&'static str; 7],
    pub weekdays_short: [&'static str; 7],
    /// Today, tomorrow, +1 week, no date.
    pub chips: [&'static str; 4],
    /// High, medium, low, no priority.
    pub priorities: [&'static str; 4],
    pub ui: &'static Ui,
}

/// Interface strings (Spanish or English).
#[derive(Debug)]
pub struct Ui {
    pub title: &'static str,
    pub done: &'static str,
    pub pending: &'static str,
    pub sort_due: &'static str,
    pub sort_priority: &'static str,
    pub theme: &'static str,
    pub light: &'static str,
    pub dark: &'static str,
    pub auto: &'static str,
    pub auto_tip: &'static str,
    pub help: &'static str,
    pub new_task: &'static str,
    pub due_placeholder: &'static str,
    pub priority: &'static str,
    pub no_priority_lower: &'static str,
    pub date: &'static str,
    pub edit: &'static str,
    pub delete: &'static str,
    pub complete: &'static str,
    pub reopen: &'static str,
    pub close: &'static str,
    pub empty: &'static str,
    pub overdue: &'static str,
    pub today: &'static str,
    pub tomorrow: &'static str,
    pub read_only: &'static str,
    pub all_done: &'static str,
    pub tray_tip: &'static str,
    pub updated: &'static str,
    pub menu_open: &'static str,
    pub menu_toggle_bar: &'static str,
    pub menu_autostart: &'static str,
    pub menu_import: &'static str,
    pub menu_export: &'static str,
    pub menu_reset: &'static str,
    pub menu_theme: &'static str,
    pub menu_system: &'static str,
    pub menu_auto_update: &'static str,
    pub menu_check: &'static str,
    pub menu_about: &'static str,
    pub menu_quit: &'static str,
    pub up_to_date: &'static str,
    pub update_failed: &'static str,
    pub imported: &'static str,
}

impl Texts {
    pub fn month_title(&self, year: i32, month: u32) -> String {
        format!("{} {year}", self.months[(month - 1) as usize])
    }

    pub fn priority_name(&self, priority: Option<&str>) -> &'static str {
        match priority {
            Some("H") => self.priorities[0],
            Some("M") => self.priorities[1],
            Some("L") => self.priorities[2],
            _ => "",
        }
    }

    /// A picked day as the date field shows it: `Thu Nov 26` / `jue 26 nov` (with the year
    /// when it is not this year). Storage keeps ISO / Taskwarrior dates.
    pub fn short_date(&self, day: NaiveDate, today: NaiveDate) -> String {
        use chrono::Datelike;
        let month = self.months_short[day.month0() as usize];
        let weekday = self.weekdays_short[weekday_index(day)];
        let dmy = !std::ptr::eq(self.ui, &UI_EN);
        match (dmy, day.year() == today.year()) {
            (false, true) => format!("{weekday} {month} {}", day.day()),
            (false, false) => format!("{month} {}, {}", day.day(), day.year()),
            (true, true) => format!("{weekday} {} {month}", day.day()),
            (true, false) => format!("{} {month} {}", day.day(), day.year()),
        }
    }

    /// The due badge: `overdue · Nov 23`, `today`, `tomorrow`, `Fri Nov 27`, `Jan 5, 2027`
    /// (Spanish: `vencida · 23 nov`, `hoy`, `mañana`, `vie 27 nov`, `5 ene 2027`).
    pub fn due_label(&self, day: NaiveDate, today: NaiveDate) -> String {
        use chrono::Datelike;
        let month = self.months_short[day.month0() as usize];
        let other_year = day.year() != today.year();
        let dmy = !std::ptr::eq(self.ui, &UI_EN);
        let short = match (dmy, other_year) {
            (false, false) => format!("{month} {}", day.day()),
            (false, true) => format!("{month} {}, {}", day.day(), day.year()),
            (true, false) => format!("{} {month}", day.day()),
            (true, true) => format!("{} {month} {}", day.day(), day.year()),
        };
        let diff = day_diff(day, today);
        if diff < 0 {
            format!("{} · {short}", self.ui.overdue)
        } else if diff == 0 {
            self.ui.today.to_owned()
        } else if diff == 1 {
            self.ui.tomorrow.to_owned()
        } else if other_year {
            short
        } else {
            format!("{} {short}", self.weekdays_short[weekday_index(day)])
        }
    }
}

static UI_EN: Ui = Ui {
    title: "Today's tasks",
    done: "Done",
    pending: "Pending",
    sort_due: "Sort: due date",
    sort_priority: "Sort: priority",
    theme: "Theme",
    light: "Light",
    dark: "Dark",
    auto: "Auto",
    auto_tip: "Follow the Windows theme",
    help: "Enter saves · Esc closes\nDate: calendar or today, +3d,\n2026-12-01 · due:+2w\npriority:H · project:X",
    new_task: "New task and Enter…",
    due_placeholder: "due",
    priority: "Priority",
    no_priority_lower: "no priority",
    date: "Due date",
    edit: "Edit",
    delete: "Delete",
    complete: "Mark as done",
    reopen: "Mark as pending",
    close: "Close",
    empty: "No tasks. Add the first one above.",
    overdue: "overdue",
    today: "today",
    tomorrow: "tomorrow",
    read_only: "Read-only: the task file could not be read safely and was left untouched.",
    all_done: "All done for today",
    tray_tip: "done today",
    updated: "Updated to",
    menu_open: "Open tasks",
    menu_toggle_bar: "Show/hide bar",
    menu_autostart: "Start with Windows",
    menu_import: "Import Taskwarrior JSON…",
    menu_export: "Export Taskwarrior JSON…",
    menu_reset: "Reset bar position",
    menu_theme: "Theme",
    menu_system: "System",
    menu_auto_update: "Update automatically",
    menu_check: "Check for updates now",
    menu_about: "About Taskline",
    menu_quit: "Quit",
    up_to_date: "is up to date.",
    update_failed: "Update not installed",
    imported: "Imported tasks:",
};

static UI_ES: Ui = Ui {
    title: "Tareas de hoy",
    done: "Hechas",
    pending: "Pendientes",
    sort_due: "Orden: fecha",
    sort_priority: "Orden: prioridad",
    theme: "Tema",
    light: "Claro",
    dark: "Oscuro",
    auto: "Auto",
    auto_tip: "Seguir el tema de Windows",
    help: "Enter guarda · Esc cierra\nFecha: calendario o hoy, +3d,\n2026-12-01 · due:+2w\npriority:H · project:X",
    new_task: "Nueva tarea y Enter…",
    due_placeholder: "fecha",
    priority: "Prioridad",
    no_priority_lower: "sin prioridad",
    date: "Fecha",
    edit: "Editar",
    delete: "Eliminar",
    complete: "Marcar como hecha",
    reopen: "Marcar como pendiente",
    close: "Cerrar",
    empty: "Sin tareas. Agrega la primera arriba.",
    overdue: "vencida",
    today: "hoy",
    tomorrow: "mañana",
    read_only: "Solo lectura: el archivo de tareas no se pudo leer con seguridad y no se modificó.",
    all_done: "Todo hecho por hoy",
    tray_tip: "hechas hoy",
    updated: "Actualizado a",
    menu_open: "Abrir tareas",
    menu_toggle_bar: "Mostrar/ocultar barra",
    menu_autostart: "Iniciar con Windows",
    menu_import: "Importar JSON de Taskwarrior…",
    menu_export: "Exportar JSON de Taskwarrior…",
    menu_reset: "Restablecer posición de la barra",
    menu_theme: "Tema",
    menu_system: "Sistema",
    menu_auto_update: "Actualizar automáticamente",
    menu_check: "Buscar actualizaciones ahora",
    menu_about: "Acerca de Taskline",
    menu_quit: "Salir",
    up_to_date: "está actualizado.",
    update_failed: "Actualización no instalada",
    imported: "Tareas importadas:",
};

static EN: Texts = Texts {
    months: [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ],
    months_short: [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ],
    weekdays: ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"],
    weekdays_short: ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"],
    chips: ["Today", "Tomorrow", "+1 week", "No date"],
    priorities: ["High", "Medium", "Low", "No priority"],
    ui: &UI_EN,
};

static ES: Texts = Texts {
    months: [
        "enero",
        "febrero",
        "marzo",
        "abril",
        "mayo",
        "junio",
        "julio",
        "agosto",
        "septiembre",
        "octubre",
        "noviembre",
        "diciembre",
    ],
    months_short: [
        "ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic",
    ],
    weekdays: ["lu", "ma", "mi", "ju", "vi", "sá", "do"],
    weekdays_short: ["lun", "mar", "mié", "jue", "vie", "sáb", "dom"],
    chips: ["Hoy", "Mañana", "+1 semana", "Sin fecha"],
    priorities: ["Alta", "Media", "Baja", "Sin prioridad"],
    ui: &UI_ES,
};

static PT: Texts = Texts {
    months: [
        "janeiro",
        "fevereiro",
        "março",
        "abril",
        "maio",
        "junho",
        "julho",
        "agosto",
        "setembro",
        "outubro",
        "novembro",
        "dezembro",
    ],
    months_short: [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ],
    weekdays: ["seg", "ter", "qua", "qui", "sex", "sáb", "dom"],
    weekdays_short: ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"],
    chips: ["Hoje", "Amanhã", "+1 semana", "Sem data"],
    priorities: ["Alta", "Média", "Baixa", "Sem prioridade"],
    ui: &UI_EN,
};

static FR: Texts = Texts {
    months: [
        "janvier",
        "février",
        "mars",
        "avril",
        "mai",
        "juin",
        "juillet",
        "août",
        "septembre",
        "octobre",
        "novembre",
        "décembre",
    ],
    months_short: [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ],
    weekdays: ["lu", "ma", "me", "je", "ve", "sa", "di"],
    weekdays_short: ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"],
    chips: ["Aujourd'hui", "Demain", "+1 semaine", "Sans date"],
    priorities: ["Haute", "Moyenne", "Basse", "Sans priorité"],
    ui: &UI_EN,
};

static DE: Texts = Texts {
    months: [
        "Januar",
        "Februar",
        "März",
        "April",
        "Mai",
        "Juni",
        "Juli",
        "August",
        "September",
        "Oktober",
        "November",
        "Dezember",
    ],
    months_short: [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ],
    weekdays: ["Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"],
    weekdays_short: ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"],
    chips: ["Heute", "Morgen", "+1 Woche", "Kein Datum"],
    priorities: ["Hoch", "Mittel", "Niedrig", "Keine Priorität"],
    ui: &UI_EN,
};

#[cfg(test)]
mod tests {
    use super::*;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn language_from_tags_and_langids() {
        assert_eq!(Language::from_tag("es_CO"), Language::Es);
        assert_eq!(Language::from_tag("pt-BR"), Language::Pt);
        assert_eq!(Language::from_tag("de_DE.UTF-8"), Language::De);
        assert_eq!(Language::from_tag("ja_JP"), Language::En);
        assert_eq!(Language::from_tag(""), Language::En);
        assert_eq!(Language::from_langid(0x240a), Language::Es); // es-CO
        assert_eq!(Language::from_langid(0x0c0a), Language::Es); // es-ES
        assert_eq!(Language::from_langid(0x0409), Language::En);
        assert_eq!(Language::from_langid(0x0416), Language::Pt);
        assert_eq!(Language::from_langid(0x0411), Language::En); // Japanese falls back
    }

    #[test]
    fn due_labels_in_english_like_v1() {
        let t = Language::En.texts();
        let today = day(2026, 11, 24);
        assert_eq!(t.due_label(day(2026, 11, 23), today), "overdue · Nov 23");
        assert_eq!(t.due_label(day(2025, 12, 30), today), "overdue · Dec 30, 2025");
        assert_eq!(t.due_label(today, today), "today");
        assert_eq!(t.due_label(day(2026, 11, 25), today), "tomorrow");
        assert_eq!(t.due_label(day(2026, 11, 27), today), "Fri Nov 27");
        assert_eq!(t.due_label(day(2027, 1, 5), today), "Jan 5, 2027");
    }

    #[test]
    fn due_labels_in_spanish_like_omarchy() {
        let t = Language::Es.texts();
        let today = day(2026, 11, 24);
        assert_eq!(t.due_label(day(2026, 11, 23), today), "vencida · 23 nov");
        assert_eq!(t.due_label(today, today), "hoy");
        assert_eq!(t.due_label(day(2026, 11, 25), today), "mañana");
        assert_eq!(t.due_label(day(2026, 11, 27), today), "vie 27 nov");
        assert_eq!(t.due_label(day(2027, 1, 5), today), "5 ene 2027");
        assert_eq!(t.month_title(2026, 11), "noviembre 2026");
        assert_eq!(t.short_date(day(2026, 11, 26), today), "jue 26 nov");
        assert_eq!(t.short_date(day(2027, 1, 5), today), "5 ene 2027");
        let en = Language::En.texts();
        assert_eq!(en.short_date(day(2026, 11, 26), today), "Thu Nov 26");
        assert_eq!(en.short_date(day(2027, 1, 5), today), "Jan 5, 2027");
        assert_eq!(t.priority_name(Some("M")), "Media");
        assert_eq!(t.priority_name(None), "");
    }
}
