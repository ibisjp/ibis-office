// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (c) ibis inc.
// Additional terms under AGPL-3.0 section 7(b): see NOTICE.
//! The names a number format's locale (`[$-407]`: the LCID after the `-`, or a tag such as `de-DE`) gives
//! to months, days and AM/PM in dates, from the locales' own calendars (Windows / CLDR names).
//!
//! English (`[$-409]`, `[$-809]`) and Japanese (`[$-411]`, with its eras and numerals) are the format
//! engine's own; the languages here put their names for `mmm`, `mmmm`, `ddd`, `dddd` and AM/PM.

/// The names of a language's calendar.
pub(crate) struct Names {
    pub months: [&'static str; 12],
    pub months_short: [&'static str; 12],
    pub days: [&'static str; 7],
    pub days_short: [&'static str; 7],
    /// The morning and afternoon marks, when the language has its own (otherwise AM and PM).
    pub am_pm: Option<(&'static str, &'static str)>,
}

/// Japanese (`[$-411]`): the month and the day names a Japanese locale gives `mmm`, `mmmm`, `ddd`, `dddd`, as Excel
/// shows them (measured on Excel 16.0 build 20527: `mmmm` 10月, `mmm` 10, `ddd` 水, `dddd` 水曜日).
const JA: Names = Names {
    months: ["1月", "2月", "3月", "4月", "5月", "6月", "7月", "8月", "9月", "10月", "11月", "12月"],
    months_short: ["1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12"],
    days: ["日曜日", "月曜日", "火曜日", "水曜日", "木曜日", "金曜日", "土曜日"],
    days_short: ["日", "月", "火", "水", "木", "金", "土"],
    am_pm: Some(("午前", "午後")),
};

const DE: Names = Names {
    months: ["Januar", "Februar", "März", "April", "Mai", "Juni", "Juli", "August", "September", "Oktober", "November", "Dezember"],
    months_short: ["Jan", "Feb", "Mrz", "Apr", "Mai", "Jun", "Jul", "Aug", "Sep", "Okt", "Nov", "Dez"],
    days: ["Sonntag", "Montag", "Dienstag", "Mittwoch", "Donnerstag", "Freitag", "Samstag"],
    days_short: ["So", "Mo", "Di", "Mi", "Do", "Fr", "Sa"],
    am_pm: None,
};

const FR: Names = Names {
    months: ["janvier", "février", "mars", "avril", "mai", "juin", "juillet", "août", "septembre", "octobre", "novembre", "décembre"],
    months_short: ["janv.", "févr.", "mars", "avr.", "mai", "juin", "juil.", "août", "sept.", "oct.", "nov.", "déc."],
    days: ["dimanche", "lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi"],
    days_short: ["dim.", "lun.", "mar.", "mer.", "jeu.", "ven.", "sam."],
    am_pm: None,
};

const ES: Names = Names {
    months: ["enero", "febrero", "marzo", "abril", "mayo", "junio", "julio", "agosto", "septiembre", "octubre", "noviembre", "diciembre"],
    months_short: ["ene", "feb", "mar", "abr", "may", "jun", "jul", "ago", "sep", "oct", "nov", "dic"],
    days: ["domingo", "lunes", "martes", "miércoles", "jueves", "viernes", "sábado"],
    days_short: ["dom", "lun", "mar", "mié", "jue", "vie", "sáb"],
    am_pm: None,
};

const IT: Names = Names {
    months: ["gennaio", "febbraio", "marzo", "aprile", "maggio", "giugno", "luglio", "agosto", "settembre", "ottobre", "novembre", "dicembre"],
    months_short: ["gen", "feb", "mar", "apr", "mag", "giu", "lug", "ago", "set", "ott", "nov", "dic"],
    days: ["domenica", "lunedì", "martedì", "mercoledì", "giovedì", "venerdì", "sabato"],
    days_short: ["dom", "lun", "mar", "mer", "gio", "ven", "sab"],
    am_pm: None,
};

const PT: Names = Names {
    months: ["janeiro", "fevereiro", "março", "abril", "maio", "junho", "julho", "agosto", "setembro", "outubro", "novembro", "dezembro"],
    months_short: ["jan", "fev", "mar", "abr", "mai", "jun", "jul", "ago", "set", "out", "nov", "dez"],
    days: ["domingo", "segunda-feira", "terça-feira", "quarta-feira", "quinta-feira", "sexta-feira", "sábado"],
    days_short: ["dom", "seg", "ter", "qua", "qui", "sex", "sáb"],
    am_pm: None,
};

const KO: Names = Names {
    months: ["1월", "2월", "3월", "4월", "5월", "6월", "7월", "8월", "9월", "10월", "11월", "12월"],
    months_short: ["1월", "2월", "3월", "4월", "5월", "6월", "7월", "8월", "9월", "10월", "11월", "12월"],
    days: ["일요일", "월요일", "화요일", "수요일", "목요일", "금요일", "토요일"],
    days_short: ["일", "월", "화", "수", "목", "금", "토"],
    am_pm: Some(("오전", "오후")),
};

const ZH_CN: Names = Names {
    months: ["一月", "二月", "三月", "四月", "五月", "六月", "七月", "八月", "九月", "十月", "十一月", "十二月"],
    months_short: ["1月", "2月", "3月", "4月", "5月", "6月", "7月", "8月", "9月", "10月", "11月", "12月"],
    days: ["星期日", "星期一", "星期二", "星期三", "星期四", "星期五", "星期六"],
    days_short: ["周日", "周一", "周二", "周三", "周四", "周五", "周六"],
    am_pm: Some(("上午", "下午")),
};

const ZH_TW: Names = Names {
    months: ["一月", "二月", "三月", "四月", "五月", "六月", "七月", "八月", "九月", "十月", "十一月", "十二月"],
    months_short: ["1月", "2月", "3月", "4月", "5月", "6月", "7月", "8月", "9月", "10月", "11月", "12月"],
    days: ["星期日", "星期一", "星期二", "星期三", "星期四", "星期五", "星期六"],
    days_short: ["週日", "週一", "週二", "週三", "週四", "週五", "週六"],
    am_pm: Some(("上午", "下午")),
};

const RU: Names = Names {
    months: ["январь", "февраль", "март", "апрель", "май", "июнь", "июль", "август", "сентябрь", "октябрь", "ноябрь", "декабрь"],
    months_short: ["янв", "фев", "мар", "апр", "май", "июн", "июл", "авг", "сен", "окт", "ноя", "дек"],
    days: ["воскресенье", "понедельник", "вторник", "среда", "четверг", "пятница", "суббота"],
    days_short: ["вс", "пн", "вт", "ср", "чт", "пт", "сб"],
    am_pm: None,
};

const NL: Names = Names {
    months: ["januari", "februari", "maart", "april", "mei", "juni", "juli", "augustus", "september", "oktober", "november", "december"],
    months_short: ["jan", "feb", "mrt", "apr", "mei", "jun", "jul", "aug", "sep", "okt", "nov", "dec"],
    days: ["zondag", "maandag", "dinsdag", "woensdag", "donderdag", "vrijdag", "zaterdag"],
    days_short: ["zo", "ma", "di", "wo", "do", "vr", "za"],
    am_pm: None,
};

const PL: Names = Names {
    months: ["styczeń", "luty", "marzec", "kwiecień", "maj", "czerwiec", "lipiec", "sierpień", "wrzesień", "październik", "listopad", "grudzień"],
    months_short: ["sty", "lut", "mar", "kwi", "maj", "cze", "lip", "sie", "wrz", "paź", "lis", "gru"],
    days: ["niedziela", "poniedziałek", "wtorek", "środa", "czwartek", "piątek", "sobota"],
    days_short: ["ndz", "pon", "wt", "śr", "czw", "pt", "sob"],
    am_pm: None,
};

const SV: Names = Names {
    months: ["januari", "februari", "mars", "april", "maj", "juni", "juli", "augusti", "september", "oktober", "november", "december"],
    months_short: ["jan", "feb", "mar", "apr", "maj", "jun", "jul", "aug", "sep", "okt", "nov", "dec"],
    days: ["söndag", "måndag", "tisdag", "onsdag", "torsdag", "fredag", "lördag"],
    days_short: ["sön", "mån", "tis", "ons", "tors", "fre", "lör"],
    am_pm: None,
};

const TR: Names = Names {
    months: ["Ocak", "Şubat", "Mart", "Nisan", "Mayıs", "Haziran", "Temmuz", "Ağustos", "Eylül", "Ekim", "Kasım", "Aralık"],
    months_short: ["Oca", "Şub", "Mar", "Nis", "May", "Haz", "Tem", "Ağu", "Eyl", "Eki", "Kas", "Ara"],
    days: ["Pazar", "Pazartesi", "Salı", "Çarşamba", "Perşembe", "Cuma", "Cumartesi"],
    days_short: ["Paz", "Pzt", "Sal", "Çar", "Per", "Cum", "Cmt"],
    am_pm: None,
};

const AR: Names = Names {
    months: ["يناير", "فبراير", "مارس", "أبريل", "مايو", "يونيو", "يوليو", "أغسطس", "سبتمبر", "أكتوبر", "نوفمبر", "ديسمبر"],
    months_short: ["يناير", "فبراير", "مارس", "أبريل", "مايو", "يونيو", "يوليو", "أغسطس", "سبتمبر", "أكتوبر", "نوفمبر", "ديسمبر"],
    days: ["الأحد", "الإثنين", "الثلاثاء", "الأربعاء", "الخميس", "الجمعة", "السبت"],
    days_short: ["أحد", "إثنين", "ثلاثاء", "أربعاء", "خميس", "جمعة", "سبت"],
    am_pm: Some(("ص", "م")),
};

const HI: Names = Names {
    months: ["जनवरी", "फ़रवरी", "मार्च", "अप्रैल", "मई", "जून", "जुलाई", "अगस्त", "सितंबर", "अक्टूबर", "नवंबर", "दिसंबर"],
    months_short: ["जन॰", "फ़र॰", "मार्च", "अप्रैल", "मई", "जून", "जुल॰", "अग॰", "सित॰", "अक्तू॰", "नव॰", "दिस॰"],
    days: ["रविवार", "सोमवार", "मंगलवार", "बुधवार", "गुरुवार", "शुक्रवार", "शनिवार"],
    days_short: ["रवि", "सोम", "मंगल", "बुध", "गुरु", "शुक्र", "शनि"],
    am_pm: Some(("पूर्वाह्न", "अपराह्न")),
};

/// The LCID a locale tag in `[$-…]` names: hexadecimal digits (`407`, `0407`), or a language tag (`de-DE`, `fr`).
pub(crate) fn lcid_of(tag: &str) -> Option<u16> {
    let t = tag.trim().to_ascii_lowercase();
    if !t.is_empty() && t.len() <= 4 && t.chars().all(|c| c.is_ascii_hexdigit()) {
        return u16::from_str_radix(&t, 16).ok();
    }
    // `[$-1010411]`: the calendar and number type in the high word; the locale is the low word.
    if (5..=8).contains(&t.len()) && t.chars().all(|c| c.is_ascii_hexdigit()) {
        return u32::from_str_radix(&t, 16).ok().map(|v| (v & 0xFFFF) as u16);
    }
    Some(match t.as_str() {
        "en" | "en-us" => 0x0409,
        "en-gb" => 0x0809,
        "ja" | "ja-jp" => 0x0411,
        "de" | "de-de" | "de-at" | "de-ch" => 0x0407,
        "fr" | "fr-fr" | "fr-ca" | "fr-be" | "fr-ch" => 0x040C,
        "es" | "es-es" | "es-mx" => 0x0C0A,
        "it" | "it-it" => 0x0410,
        "pt" | "pt-br" => 0x0416,
        "pt-pt" => 0x0816,
        "ko" | "ko-kr" => 0x0412,
        "zh" | "zh-cn" | "zh-hans" => 0x0804,
        "zh-tw" | "zh-hant" => 0x0404,
        "ru" | "ru-ru" => 0x0419,
        "nl" | "nl-nl" | "nl-be" => 0x0413,
        "pl" | "pl-pl" => 0x0415,
        "sv" | "sv-se" | "sv-fi" => 0x041D,
        "tr" | "tr-tr" => 0x041F,
        "ar" | "ar-sa" | "ar-eg" => 0x0401,
        "hi" | "hi-in" => 0x0439,
        _ => return None,
    })
}

/// The names of the language of `lcid`, `None` for English and the languages this does not name (English names).
/// The Japanese ones are used by `[$-411]`; a format without a locale shows English `ddd`/`dddd` and Japanese `aaa`/`aaaa`.
pub(crate) fn names(lcid: u16) -> Option<&'static Names> {
    // The low 10 bits are the language; the region is above.
    match lcid & 0x3FF {
        0x11 => Some(&JA),
        0x07 => Some(&DE),
        0x0C => Some(&FR),
        0x0A => Some(&ES),
        0x10 => Some(&IT),
        0x16 => Some(&PT),
        0x12 => Some(&KO),
        0x04 if lcid == 0x0804 || lcid == 0x1004 => Some(&ZH_CN),
        0x04 => Some(&ZH_TW),
        0x19 => Some(&RU),
        0x13 => Some(&NL),
        0x15 => Some(&PL),
        0x1D => Some(&SV),
        0x1F => Some(&TR),
        0x01 => Some(&AR),
        0x39 => Some(&HI),
        _ => None,
    }
}
