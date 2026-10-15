// Monaco language + themes for .m code.
import type * as Monaco from "monaco-editor";

export const LANG = "mcode";

export function registerLanguage(monaco: typeof Monaco) {
  monaco.languages.register({ id: LANG, extensions: [".m"], aliases: ["MATLAB-language", "m"] });
  monaco.languages.setLanguageConfiguration(LANG, {
    comments: { lineComment: "%", blockComment: ["%{", "%}"] },
    brackets: [["(", ")"], ["[", "]"], ["{", "}"]],
    autoClosingPairs: [
      { open: "(", close: ")" },
      { open: "[", close: "]" },
      { open: "{", close: "}" },
      { open: '"', close: '"', notIn: ["string"] },
    ],
    indentationRules: {
      increaseIndentPattern: /^\s*(function|if|elseif|else|for|parfor|while|switch|case|otherwise|try|catch)\b.*$/,
      decreaseIndentPattern: /^\s*(end|elseif|else|case|otherwise|catch)\b.*$/,
    },
    onEnterRules: [],
  });
  monaco.languages.setMonarchTokensProvider(LANG, {
    keywords: [
      "if", "elseif", "else", "end", "for", "parfor", "while", "switch", "case", "otherwise",
      "try", "catch", "function", "return", "break", "continue", "global", "persistent",
    ],
    constants: ["true", "false", "pi", "Inf", "inf", "NaN", "nan", "eps", "end"],
    tokenizer: {
      root: [
        [/^\s*%%.*$/, "comment.section"],
        [/^\s*%\{\s*$/, { token: "comment", next: "@blockComment" }],
        [/%.*$/, "comment"],
        [/\.\.\..*$/, "comment"],
        [/[a-zA-Z_]\w*/, { cases: { "@keywords": "keyword", "@constants": "constant", "@default": "identifier" } }],
        [/\d+(\.\d*)?([eEdD][+-]?\d+)?|\.\d+([eEdD][+-]?\d+)?/, "number"],
        [/"/, { token: "string", next: "@dstring" }],
        // A quote after a value is transpose; otherwise it starts a char literal.
        [/(?<=[\w\)\]\}\.'])'/, "operator"],
        [/'/, { token: "string", next: "@sstring" }],
        [/[=<>~!&|+\-*\/\\^.:@]+/, "operator"],
        [/[;,]/, "delimiter"],
        [/[()\[\]{}]/, "@brackets"],
      ],
      blockComment: [
        [/^\s*%\}\s*$/, { token: "comment", next: "@pop" }],
        [/.*$/, "comment"],
      ],
      sstring: [
        [/''/, "string"],
        [/'/, { token: "string", next: "@pop" }],
        [/[^']+/, "string"],
      ],
      dstring: [
        [/""/, "string"],
        [/"/, { token: "string", next: "@pop" }],
        [/[^"]+/, "string"],
      ],
    },
  } as any);

  monaco.editor.defineTheme("beany-light", {
    base: "vs",
    inherit: true,
    rules: [
      { token: "keyword", foreground: "3730A3", fontStyle: "bold" },
      { token: "comment", foreground: "15803D" },
      { token: "comment.section", foreground: "15803D", fontStyle: "bold" },
      { token: "string", foreground: "A21CAF" },
      { token: "number", foreground: "0F766E" },
      { token: "constant", foreground: "0F766E" },
      { token: "operator", foreground: "334155" },
    ],
    colors: {
      "editor.background": "#FFFFFF",
      "editorLineNumber.foreground": "#94A3B8",
      "editorLineNumber.activeForeground": "#312E81",
      "editor.lineHighlightBackground": "#F1F5F9",
      "editor.selectionBackground": "#C7D2FE",
      "editorCursor.foreground": "#312E81",
    },
  });
  monaco.editor.defineTheme("beany-dark", {
    base: "vs-dark",
    inherit: true,
    rules: [
      { token: "keyword", foreground: "A5B4FC", fontStyle: "bold" },
      { token: "comment", foreground: "6EE7B7" },
      { token: "comment.section", foreground: "6EE7B7", fontStyle: "bold" },
      { token: "string", foreground: "F0ABFC" },
      { token: "number", foreground: "5EEAD4" },
      { token: "constant", foreground: "5EEAD4" },
    ],
    colors: {
      "editor.background": "#16162A",
      "editor.lineHighlightBackground": "#1E1E38",
      "editorLineNumber.foreground": "#5B5F86",
      "editor.selectionBackground": "#3730A380",
    },
  });
}

/** Line ranges of %% sections; returns [startLine, endLine] (1-based, inclusive) containing `line`. */
export function sectionAt(text: string, line: number): [number, number] {
  const lines = text.split(/\r?\n/);
  let start = 1;
  for (let i = line; i >= 1; i--) {
    if (/^\s*%%/.test(lines[i - 1] ?? "")) {
      start = i;
      break;
    }
  }
  let end = lines.length;
  for (let i = line + 1; i <= lines.length; i++) {
    if (/^\s*%%/.test(lines[i - 1] ?? "")) {
      end = i - 1;
      break;
    }
  }
  return [start, end];
}
