import js from "@eslint/js";
import reactHooks from "eslint-plugin-react-hooks";
import globals from "globals";
import tseslint from "typescript-eslint";

/**
 * The front end's lint: the recommended rules for TypeScript, and the rules of
 * hooks — the one class of React bug the type checker cannot see.
 *
 * Only the two classic hook rules: the plugin's newer ones are written for the
 * React Compiler, which this app does not use, and they forbid the "latest
 * value in a ref" pattern it relies on on purpose.
 */
export default tseslint.config(
  { ignores: ["dist", "src-tauri", "node_modules"] },
  {
    files: ["src/**/*.{ts,tsx}", "*.ts", "*.js"],
    extends: [js.configs.recommended, ...tseslint.configs.recommended],
    languageOptions: {
      ecmaVersion: 2023,
      globals: { ...globals.browser, ...globals.node },
    },
    plugins: { "react-hooks": reactHooks },
    rules: {
      "react-hooks/rules-of-hooks": "error",
      "react-hooks/exhaustive-deps": "error",
    },
  },
);
