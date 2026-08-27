//! GEDCOM tags used by the core.


pub const MREL: &str = "_MREL";
pub const FREL: &str = "_FREL";
pub const BIRTH: &str = "BIRT";
pub const BURIAL: &str = "BURI";
pub const CENSUS: &str = "CENS";
pub const CHANGE: &str = "CHAN";
pub const CHILD: &str = "CHIL";
pub const CONCATENATION: &str = "CONC";
pub const CONTINUED: &str = "CONT";
pub const DATE: &str = "DATE";
pub const DEATH: &str = "DEAT";
pub const FAMILY: &str = "FAM";
pub const FAMILY_CHILD: &str = "FAMC";
pub const FAMILY_SPOUSE: &str = "FAMS";
pub const FILE: &str = "FILE";
pub const GIVEN_NAME: &str = "GIVN";
pub const HUSBAND: &str = "HUSB";
pub const INDIVIDUAL: &str = "INDI";
pub const MARRIAGE: &str = "MARR";
pub const NAME: &str = "NAME";
pub const OBJECT: &str = "OBJE";
pub const OCCUPATION: &str = "OCCU";
pub const PLACE: &str = "PLAC";
pub const PRIVATE: &str = "PRIV";
pub const SEX: &str = "SEX";
pub const SOURCE: &str = "SOUR";
pub const SURNAME: &str = "SURN";
pub const WIFE: &str = "WIFE";

pub const MEMBERS_ALL: &str = "ALL";
pub const MEMBERS_PARENTS: &str = "PARENTS";

/// Every tag above, paired with the `gedcom.tags` name it mirrors.
///
/// The selectors are absent: they are not GEDCOM tags.
pub const ALL: &[(&str, &str)] = &[
    ("GEDCOM_PROGRAM_DEFINED_TAG_MREL", MREL),
    ("GEDCOM_PROGRAM_DEFINED_TAG_FREL", FREL),
    ("GEDCOM_TAG_BIRTH", BIRTH),
    ("GEDCOM_TAG_BURIAL", BURIAL),
    ("GEDCOM_TAG_CENSUS", CENSUS),
    ("GEDCOM_TAG_CHANGE", CHANGE),
    ("GEDCOM_TAG_CHILD", CHILD),
    ("GEDCOM_TAG_CONCATENATION", CONCATENATION),
    ("GEDCOM_TAG_CONTINUED", CONTINUED),
    ("GEDCOM_TAG_DATE", DATE),
    ("GEDCOM_TAG_DEATH", DEATH),
    ("GEDCOM_TAG_FAMILY", FAMILY),
    ("GEDCOM_TAG_FAMILY_CHILD", FAMILY_CHILD),
    ("GEDCOM_TAG_FAMILY_SPOUSE", FAMILY_SPOUSE),
    ("GEDCOM_TAG_FILE", FILE),
    ("GEDCOM_TAG_GIVEN_NAME", GIVEN_NAME),
    ("GEDCOM_TAG_HUSBAND", HUSBAND),
    ("GEDCOM_TAG_INDIVIDUAL", INDIVIDUAL),
    ("GEDCOM_TAG_MARRIAGE", MARRIAGE),
    ("GEDCOM_TAG_NAME", NAME),
    ("GEDCOM_TAG_OBJECT", OBJECT),
    ("GEDCOM_TAG_OCCUPATION", OCCUPATION),
    ("GEDCOM_TAG_PLACE", PLACE),
    ("GEDCOM_TAG_PRIVATE", PRIVATE),
    ("GEDCOM_TAG_SEX", SEX),
    ("GEDCOM_TAG_SOURCE", SOURCE),
    ("GEDCOM_TAG_SURNAME", SURNAME),
    ("GEDCOM_TAG_WIFE", WIFE),
];
