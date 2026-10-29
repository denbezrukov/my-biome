        (
            Types,
            vec![
                ("index.ts", "import './provider';"),
                ("provider.ts", "export interface Shape {}"),
            ],
            vec![
                (
                    "index.ts",
                    "import type {Shape} from './provider'; type Used = Shape;",
                ),
                ("provider.ts", "export interface Shape {}"),
            ],
        ),
        (
            NsExports,
