import type MarkdownIt from 'markdown-it';
import type Token from 'markdown-it/lib/token.mjs';
import { type SupramarkParentNode } from '../ast.js';
import { type SupramarkConfig } from '../feature.js';
/**
 * Input 语法处理上下文。
 *
 * - sourceLines: 按行拆分的原始 Markdown 文本；
 * - stack: 当前 AST 父节点栈（与 parseMarkdown 中保持一致）。
 */
export interface InputProcessorContext {
    config?: SupramarkConfig;
    sourceLines: string[];
    stack: SupramarkParentNode[];
}
/**
 * 供 Feature 级 input hook 使用的上下文。
 *
 * - 在 InputProcessorContext 基础上增加当前 token / name / phase。
 */
export interface InputHookContext extends InputProcessorContext {
    token: Token;
    name: string;
    phase: 'open' | 'close';
}
export interface InputHook {
    /** Input 块名称，对应 %%%name 中的 name */
    name: string;
    /**
     * 是否为"不透明容器"
     *
     * - opaque = true 时，容器内部的 token 将不会进入默认 AST 构建流程；
     * - 典型用法：%%%form 等需要直接基于原始文本进行解析的语法。
     */
    opaque?: boolean;
    onOpen: (ctx: InputHookContext) => void;
    onClose?: (ctx: InputHookContext) => void;
}
export declare function registerInputHook(hook: InputHook): void;
/**
 * 在 MarkdownIt 实例上注册 %%% input 块语法。
 *
 * 语法格式：
 * ```
 * %%%name params
 * content
 * %%%
 * ```
 */
export declare function registerInputSyntax(md: MarkdownIt, _config?: SupramarkConfig): void;
/**
 * 创建 input 语法的 AST 处理器。
 *
 * 返回一个函数，用于在 parseMarkdown 的 token 遍历中处理 input_open / input_close。
 */
export declare function createInputProcessor(ctx: InputProcessorContext): (token: Token) => boolean;
/**
 * 从 input_open token 的信息中提取内部原始文本。
 */
export declare function extractInputInnerText(token: Token, sourceLines: string[]): string;
