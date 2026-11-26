"""Calendar names in the user's Windows display language (English fallback)."""

from __future__ import annotations

import locale
import os
import sys
from dataclasses import dataclass


@dataclass(frozen=True)
class CalendarTexts:
    months: tuple[str, ...]
    weekdays: tuple[str, ...]  # Monday first
    chips: tuple[str, str, str, str]  # today, tomorrow, +1 week, no date
    priorities: tuple[str, str, str, str]  # high, medium, low, no priority

    def priority_name(self, priority: str | None) -> str:
        return {"H": self.priorities[0], "M": self.priorities[1], "L": self.priorities[2]}.get(priority or "", "")

    def month_title(self, year: int, month: int) -> str:
        return f"{self.months[month - 1]} {year}"


_TEXTS = {
    "en": CalendarTexts(
        ("January", "February", "March", "April", "May", "June", "July", "August", "September", "October",
         "November", "December"),
        ("Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"),
        ("Today", "Tomorrow", "+1 week", "No date"),
        ("High", "Medium", "Low", "No priority"),
    ),
    "es": CalendarTexts(
        ("enero", "febrero", "marzo", "abril", "mayo", "junio", "julio", "agosto", "septiembre", "octubre",
         "noviembre", "diciembre"),
        ("lu", "ma", "mi", "ju", "vi", "sá", "do"),
        ("Hoy", "Mañana", "+1 semana", "Sin fecha"),
        ("Alta", "Media", "Baja", "Sin prioridad"),
    ),
    "pt": CalendarTexts(
        ("janeiro", "fevereiro", "março", "abril", "maio", "junho", "julho", "agosto", "setembro", "outubro",
         "novembro", "dezembro"),
        ("seg", "ter", "qua", "qui", "sex", "sáb", "dom"),
        ("Hoje", "Amanhã", "+1 semana", "Sem data"),
        ("Alta", "Média", "Baixa", "Sem prioridade"),
    ),
    "fr": CalendarTexts(
        ("janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre", "octobre",
         "novembre", "décembre"),
        ("lu", "ma", "me", "je", "ve", "sa", "di"),
        ("Aujourd'hui", "Demain", "+1 semaine", "Sans date"),
        ("Haute", "Moyenne", "Basse", "Sans priorité"),
    ),
    "de": CalendarTexts(
        ("Januar", "Februar", "März", "April", "Mai", "Juni", "Juli", "August", "September", "Oktober",
         "November", "Dezember"),
        ("Mo", "Di", "Mi", "Do", "Fr", "Sa", "So"),
        ("Heute", "Morgen", "+1 Woche", "Kein Datum"),
        ("Hoch", "Mittel", "Niedrig", "Keine Priorität"),
    ),
}
LANGUAGES = tuple(_TEXTS)


def language_from_tag(tag: str | None) -> str:
    """"es_CO", "pt-BR", "de_DE.UTF-8" -> a supported language code, else "en"."""
    code = (tag or "").replace("-", "_").split("_")[0].split(".")[0].lower()
    return code if code in _TEXTS else "en"


def _windows_ui_tag() -> str | None:
    try:
        import ctypes

        lcid = ctypes.windll.kernel32.GetUserDefaultUILanguage()
    except (AttributeError, OSError):
        return None
    return locale.windows_locale.get(lcid)


def current_language() -> str:
    """TASKLINE_LANG, else the Windows display language, else the process locale."""
    override = os.environ.get("TASKLINE_LANG")
    if override:
        return language_from_tag(override)
    if sys.platform == "win32":
        tag = _windows_ui_tag()
        if tag:
            return language_from_tag(tag)
    return language_from_tag(locale.getlocale()[0] or os.environ.get("LANG"))


def calendar_texts(language: str | None = None) -> CalendarTexts:
    return _TEXTS.get(language or current_language(), _TEXTS["en"])
