//! Built-in project templates, embedded into the binary at build time.

pub struct Template {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub main: &'static str,
    pub files: &'static [(&'static str, &'static str)],
}

const REFS: &str = include_str!("../templates/common/refs.bib");

pub const TEMPLATES: &[Template] = &[
    Template {
        id: "article",
        name: "Article (plain)",
        description: "Standard article class with natbib, booktabs, hyperref.",
        main: "main.tex",
        files: &[
            ("main.tex", include_str!("../templates/article/main.tex")),
            ("refs.bib", REFS),
        ],
    },
    Template {
        id: "acm-sigconf",
        name: "ACM conference (acmart sigconf)",
        description:
            "CHI, CSCW, UIST and other ACM SIG proceedings; starts in anonymous review mode.",
        main: "main.tex",
        files: &[
            (
                "main.tex",
                include_str!("../templates/acm-sigconf/main.tex"),
            ),
            ("refs.bib", REFS),
        ],
    },
    Template {
        id: "ieee-conference",
        name: "IEEE conference (IEEEtran)",
        description: "Two-column IEEE conference paper.",
        main: "main.tex",
        files: &[
            (
                "main.tex",
                include_str!("../templates/ieee-conference/main.tex"),
            ),
            ("refs.bib", REFS),
        ],
    },
    Template {
        id: "lncs",
        name: "Springer LNCS (llncs)",
        description: "Lecture Notes in Computer Science proceedings paper.",
        main: "main.tex",
        files: &[
            ("main.tex", include_str!("../templates/lncs/main.tex")),
            ("refs.bib", REFS),
        ],
    },
    Template {
        id: "beamer",
        name: "Slides (beamer)",
        description: "16:9 beamer presentation with the Madrid theme.",
        main: "main.tex",
        files: &[("main.tex", include_str!("../templates/beamer/main.tex"))],
    },
    Template {
        id: "thesis",
        name: "Thesis (report, chapters in files)",
        description: "Report class with a title page, abstract, and one file per chapter.",
        main: "main.tex",
        files: &[
            ("main.tex", include_str!("../templates/thesis/main.tex")),
            ("refs.bib", REFS),
            (
                "chapters/introduction.tex",
                include_str!("../templates/thesis/chapters/introduction.tex"),
            ),
            (
                "chapters/background.tex",
                include_str!("../templates/thesis/chapters/background.tex"),
            ),
            (
                "chapters/method.tex",
                include_str!("../templates/thesis/chapters/method.tex"),
            ),
            (
                "chapters/results.tex",
                include_str!("../templates/thesis/chapters/results.tex"),
            ),
            (
                "chapters/discussion.tex",
                include_str!("../templates/thesis/chapters/discussion.tex"),
            ),
            (
                "chapters/conclusion.tex",
                include_str!("../templates/thesis/chapters/conclusion.tex"),
            ),
        ],
    },
];
