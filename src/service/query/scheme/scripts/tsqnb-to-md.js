#!/usr/bin/env node

const fs = require('fs');
const path = require('path');

/**
 * 将 .tsqnb 文件转换为 .md 文件
 * 
 * 使用方法:
 * node tsqnb-to-md.js <tsqnb path> [output md path]
 * 
 * 如果不指定输出路径，默认为 tsqnb 文件所在目录的上一级目录下的 docs 目录
 */

function main() {
    const args = process.argv.slice(2);
    
    if (args.length < 1) {
        console.error('Usage: node tsqnb-to-md.js <tsqnb path> [output md path]');
        process.exit(1);
    }
    
    const tsqnbPath = args[0];
    let outputPath = args[1];
    
    // 检查输入文件是否存在
    if (!fs.existsSync(tsqnbPath)) {
        console.error(`Error: File not found: ${tsqnbPath}`);
        process.exit(1);
    }
    
    // 如果没有指定输出路径，使用默认路径
    if (!outputPath) {
        const tsqnbDir = path.dirname(tsqnbPath);
        const parentDir = path.dirname(tsqnbDir);
        const docsDir = path.join(parentDir, 'docs');
        const filename = path.basename(tsqnbPath, '.tsqnb') + '.md';
        outputPath = path.join(docsDir, filename);
    }
    
    // 确保输出目录存在
    const outputDir = path.dirname(outputPath);
    if (!fs.existsSync(outputDir)) {
        fs.mkdirSync(outputDir, { recursive: true });
    }
    
    try {
        // 读取 tsqnb 文件
        const tsqnbContent = fs.readFileSync(tsqnbPath, 'utf-8');
        const data = JSON.parse(tsqnbContent);
        
        // 转换为 markdown 格式
        const markdown = convertToMarkdown(data);
        
        // 写入输出文件
        fs.writeFileSync(outputPath, markdown, 'utf-8');
        
        console.log(`Successfully converted: ${tsqnbPath} -> ${outputPath}`);
    } catch (error) {
        console.error(`Error converting file: ${error.message}`);
        process.exit(1);
    }
}

/**
 * 将 tsqnb 数据转换为 markdown 格式
 */
function convertToMarkdown(data) {
    const cells = data.cells || [];
    let markdown = '';
    let cppCode = '';
    let scmCode = '';
    let parserOutput = '';
    let resultOutput = '';
    
    // 提取C/C++代码、Scheme代码和它们的输出
    for (let i = 0; i < cells.length; i++) {
        const cell = cells[i];
        const { code, language, kind, outputs } = cell;
        
        if (kind !== 'code') {
            continue;
        }
        
        // 根据语言类型处理
        if (language === 'cpp' || language === 'c') {
            cppCode = code;
            // 提取 parser 输出
            if (outputs && outputs.length > 0) {
                const treeSitterOutput = outputs[0].items.find(item => item.mime === 'x-application/tree-sitter');
                if (treeSitterOutput && treeSitterOutput.data) {
                    const jsonString = Buffer.from(treeSitterOutput.data).toString('utf-8');
                    const jsonData = JSON.parse(jsonString);
                    parserOutput = JSON.stringify(jsonData, null, 2);
                }
            }
        } else if (language === 'scm') {
            scmCode = code;
            // 提取 result 输出
            if (outputs && outputs.length > 0) {
                const jsonOutput = outputs[0].items.find(item => item.mime === 'application/json');
                if (jsonOutput && jsonOutput.data) {
                    const jsonString = Buffer.from(jsonOutput.data).toString('utf-8');
                    const jsonData = JSON.parse(jsonString);
                    resultOutput = JSON.stringify(jsonData, null, 2);
                }
            }
        }
    }
    
    // 添加C/C++代码块
    if (cppCode) {
        markdown += '```c\n';
        markdown += cppCode;
        markdown += '\n```\n\n';
    }
    
    // 添加parser部分
    markdown += '**parser**\n\n';
    if (parserOutput) {
        markdown += '```json\n';
        markdown += parserOutput;
        markdown += '\n```\n\n';
    } else {
        markdown += '```\n';
        markdown += '// Parser output will be generated here\n';
        markdown += '```\n\n';
    }
    
    // 添加分隔符
    markdown += '---\n\n';
    
    // 添加Scheme查询代码块
    if (scmCode) {
        markdown += '```scm\n';
        markdown += scmCode;
        markdown += '\n```\n\n';
    }
    
    // 添加分隔符
    markdown += '---\n\n';
    
    // 添加result部分
    markdown += '**result**\n\n';
    if (resultOutput) {
        markdown += '```json\n';
        markdown += resultOutput;
        markdown += '\n```\n';
    } else {
        markdown += '```json\n';
        markdown += '[]\n';
        markdown += '```\n';
    }
    
    return markdown;
}

// 运行主函数
main();