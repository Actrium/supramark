/**
 * Supramark Web className system
 *
 * This file defines the className types and defaults for Supramark Web components.
 * Users can customize the className of each element by passing the classNames prop.
 */

/**
 * Customizable className keys for Supramark
 */
export interface SupramarkClassNames {
  // Block elements
  paragraph?: string;
  h1?: string;
  h2?: string;
  h3?: string;
  h4?: string;
  h5?: string;
  h6?: string;
  blockquote?: string; // blockquote element
  thematicBreak?: string; // hr element

  // Code blocks
  codeBlock?: string; // pre element
  code?: string; // code element
  /** Wrapper around a code block header (lang + button) and the pre. */
  codeBlockContainer?: string;
  /** Header row: language label on the left, copy button on the right. */
  codeBlockHeader?: string;
  /** Language label in the header (the fenced info string). */
  codeBlockLang?: string;
  /** Copy button in the header. */
  codeButton?: string;
  /** Label text inside the copy button. */
  codeButtonText?: string;

  // Lists
  listOrdered?: string; // ol element
  listUnordered?: string; // ul element
  listItem?: string; // li element
  taskListItem?: string; // li for task list items
  taskCheckbox?: string; // checkbox for task list items

  // Inline elements
  strong?: string;
  emphasis?: string;
  inlineCode?: string;
  link?: string;
  image?: string;
  delete?: string;

  // Tables
  table?: string;
  tableHead?: string; // thead element
  tableBody?: string; // tbody element
  tableRow?: string; // tr element
  tableCell?: string; // td element
  tableHeaderCell?: string; // th element

  // Diagram
  diagram?: string; // diagram container div
  diagramPre?: string; // pre inside diagram
  diagramCode?: string; // code inside diagram

  // Container
  root?: string; // outermost container
}

/**
 * Default className (empty, users are free to add their own)
 */
export const defaultClassNames: SupramarkClassNames = {
  // No className is added by default, keeping native HTML elements untouched
};

/**
 * Merge user className with default className
 * @param customClassNames user-provided className overrides
 * @returns merged className
 */
export function mergeClassNames(customClassNames?: SupramarkClassNames): SupramarkClassNames {
  if (!customClassNames) {
    return defaultClassNames;
  }

  return {
    ...defaultClassNames,
    ...customClassNames,
  };
}

/**
 * Tailwind CSS theme preset (example)
 */
export const tailwindClassNames: SupramarkClassNames = {
  root: 'prose prose-slate max-w-none',
  paragraph: 'mb-4 leading-7',
  h1: 'text-4xl font-bold mb-4 mt-6',
  h2: 'text-3xl font-semibold mb-3 mt-5',
  h3: 'text-2xl font-semibold mb-3 mt-4',
  h4: 'text-xl font-medium mb-2 mt-3',
  h5: 'text-lg font-medium mb-2 mt-3',
  h6: 'text-base font-medium mb-2 mt-2',
  blockquote: 'border-l-4 border-gray-300 dark:border-gray-600 pl-4 mb-4',
  thematicBreak: 'border-t border-gray-300 dark:border-gray-700 my-4',
  codeBlock: 'bg-gray-100 dark:bg-gray-800 rounded-md p-4 mb-4 overflow-x-auto',
  code: 'font-mono text-sm',
  codeBlockContainer: 'rounded-md mb-4 overflow-hidden',
  codeBlockHeader:
    'flex items-center justify-between px-2 py-1 bg-gray-200 dark:bg-gray-700 select-none',
  codeBlockLang: 'text-xs text-gray-600 dark:text-gray-300 font-mono select-none',
  codeButton:
    'bg-gray-700 dark:bg-gray-600 text-white text-xs rounded px-2 py-1 hover:bg-gray-600 dark:hover:bg-gray-500 select-none',
  codeButtonText: '',
  listOrdered: 'list-decimal ml-6 mb-4',
  listUnordered: 'list-disc ml-6 mb-4',
  listItem: 'mb-1',
  taskListItem: 'list-none mb-1',
  taskCheckbox: 'mr-2',
  strong: 'font-bold',
  emphasis: 'italic',
  inlineCode: 'font-mono text-sm bg-gray-100 dark:bg-gray-800 px-1.5 py-0.5 rounded',
  link: 'text-blue-600 dark:text-blue-400 hover:underline',
  image: 'max-w-full h-auto',
  delete: 'line-through',
  table: 'border-collapse border border-gray-300 dark:border-gray-700 mb-4 w-full',
  tableHead: '',
  tableBody: '',
  tableRow: 'border-b border-gray-300 dark:border-gray-700',
  tableCell: 'border border-gray-300 dark:border-gray-700 px-4 py-2',
  tableHeaderCell:
    'border border-gray-300 dark:border-gray-700 px-4 py-2 bg-gray-100 dark:bg-gray-800 font-semibold',
  diagram: 'mb-4 border border-gray-300 dark:border-gray-700 rounded-md overflow-hidden',
  diagramPre: 'p-4 bg-gray-50 dark:bg-gray-900',
  diagramCode: 'font-mono text-sm',
};

/**
 * Minimal theme preset (example)
 */
export const minimalClassNames: SupramarkClassNames = {
  root: 'supramark',
  paragraph: 'sm-p',
  h1: 'sm-h1',
  h2: 'sm-h2',
  h3: 'sm-h3',
  h4: 'sm-h4',
  h5: 'sm-h5',
  h6: 'sm-h6',
  blockquote: 'sm-blockquote',
  thematicBreak: 'sm-hr',
  codeBlock: 'sm-code-block',
  code: 'sm-code',
  codeBlockContainer: 'sm-code-block-container',
  codeBlockHeader: 'sm-code-block-header',
  codeBlockLang: 'sm-code-block-lang',
  codeButton: 'sm-code-btn',
  codeButtonText: 'sm-code-btn-text',
  listOrdered: 'sm-ol',
  listUnordered: 'sm-ul',
  listItem: 'sm-li',
  taskListItem: 'sm-task-li',
  taskCheckbox: 'sm-checkbox',
  strong: 'sm-strong',
  emphasis: 'sm-em',
  inlineCode: 'sm-inline-code',
  link: 'sm-link',
  image: 'sm-img',
  delete: 'sm-del',
  table: 'sm-table',
  tableHead: 'sm-thead',
  tableBody: 'sm-tbody',
  tableRow: 'sm-tr',
  tableCell: 'sm-td',
  tableHeaderCell: 'sm-th',
  diagram: 'sm-diagram',
  diagramPre: 'sm-diagram-pre',
  diagramCode: 'sm-diagram-code',
};
