import type MarkdownIt from 'markdown-it';
import type Token from 'markdown-it/lib/token.mjs';
import { type SupramarkParentNode } from '../ast.js';
import { type SupramarkConfig } from '../feature.js';
/**
 * 容器语法处理上下文。
 *
 * - sourceLines: 按行拆分的原始 Markdown 文本；
 * - stack: 当前 AST 父节点栈（与 parseMarkdown 中保持一致）。
 */
export interface ContainerProcessorContext {
    config?: SupramarkConfig;
    sourceLines: string[];
    stack: SupramarkParentNode[];
}
/**
 * 供 Feature 级容器 hook 使用的上下文。
 *
 * - 在 ContainerProcessorContext 基础上增加当前 token / name / phase。
 */
export interface ContainerHookContext extends ContainerProcessorContext {
    token: Token;
    name: string;
    phase: 'open' | 'close';
}
export interface ContainerHook {
    /** 容器名称，对应 :::name 中的 name */
    name: string;
    /**
     * 是否为“不透明容器”
     *
     * - opaque = true 时，容器内部的 token 将不会进入默认 AST 构建流程；
     * - 典型用法：:::map / :::html 等需要直接基于原始文本进行解析的语法。
     */
    opaque?: boolean;
    onOpen: (ctx: ContainerHookContext) => void;
    onClose?: (ctx: ContainerHookContext) => void;
}
export declare function registerContainerHook(hook: ContainerHook): void;
/**
 * 根据配置，在 MarkdownIt 实例上注册所有需要的容器语法。
 *
 * 当前支持：
 * - Admonition：::: note / ::: warning 等；
 * - HTML Page：:::html；
 * - Map：:::map；
 * - 通过 registerContainerHook() 注册的自定义容器。
 */
export declare function registerContainerSyntax(md: MarkdownIt, config?: SupramarkConfig): void;
/**
 * 创建一个容器语法 token 处理器。
 *
 * - 在 parseMarkdown 主循环中调用；
 * - 如果返回 true，表示当前 token 已被容器层消费，调用方应跳过后续处理；
 * - 支持“透明容器”（admonition）和“不透明容器”（html/map）。
 */
export declare function createContainerTokenProcessor(context: ContainerProcessorContext): (token: Token) => boolean;
/**
 * 从 container_open token 的信息中提取容器内部原始文本。
 *
 * markdown-it-container 约定：
 * - token.map[0] 为开启围栏行（`:::name`）的行号；
 * - token.map[1] 为关闭围栏行（`:::`）的行号（**不是** 之后的行号）；
 * - 内部内容行范围为 [start + 1, end)（JS slice 语义天然排除掉关闭围栏行）。
 */
export declare function extractContainerInnerText(token: Token, sourceLines: string[]): string;
