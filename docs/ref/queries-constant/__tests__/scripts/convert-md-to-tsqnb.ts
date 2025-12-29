#!/usr/bin/env node

import * as fs from 'fs';

interface TestCase {
  id: string;
 section: string;
  code: string;
  query: string;
  description: string;
  language: string;
}

interface TsqnbFormat {
  cells: Array<{
    code: string;
    language: string;
    kind: string;
  }>;
}

/**
 * 从Markdown文件中解析测试用例
 */
function parseMarkdownFile(filePath: string): TestCase[] {
  const content = fs.readFileSync(filePath, 'utf-8');
  const testCases: TestCase[] = [];

  // 按 ## \d+. 标题分割文档
  const sections = content.split(/^## \d+\.\s+/m);

  let testIndex = 1;

  for (let i = 1; i < sections.length; i++) {
    const sectionContent = sections[i];

    // 提取section标题（第一行）
    const titleMatch = sectionContent.match(/^([^\n]+)/);
    const sectionTitle = titleMatch ? titleMatch[1].trim() : `section-${i}`;

    // 提取代码块（支持多种语言标记）
    const codeBlockRegex = /```(\w+)\n([\s\S]*?)\n```/g;
    const codeMatches = [...sectionContent.matchAll(codeBlockRegex)];
    if (codeMatches.length === 0) continue;

    // 使用第一个代码块的语言标记
    const language = codeMatches[0][1].toLowerCase();
    const codeBlock = codeMatches[0][2];

    // 查找查询块：先找代码块前的查询，再找代码块后的查询
    let query = '';

    // 在代码块之前查找查询
    const preCodeContent = sectionContent.substring(0, sectionContent.indexOf(codeMatches[0][0]));
    const preQueryMatches = [...preCodeContent.matchAll(/```\n([\s\S]*?)\n```/g)];

    if (preQueryMatches.length > 0) {
      query = preQueryMatches[preQueryMatches.length - 1][1]; // 使用最后一个匹配的查询块
    } else {
      // 从代码块后查找查询
      const postCodeStart = sectionContent.indexOf(codeMatches[0][0]) + codeMatches[0][0].length;
      const postCodeContent = sectionContent.substring(postCodeStart);
      const postQueryMatches = [...postCodeContent.matchAll(/```\n([\s\S]*?)\n```/g)];
      if (postQueryMatches.length > 0) {
        query = postQueryMatches[0][1];
      }
    }

    testCases.push({
      id: String(testIndex).padStart(3, '0'),
      section: sectionTitle,
      code: codeBlock.trim(),
      query: query.trim(),
      description: `Test case ${String(testIndex).padStart(3, '0')}: ${sectionTitle}`,
      language: language
    });

    testIndex++;
  }

  return testCases;
}

/**
 * 将测试用例转换为tsqnb格式
 */
function convertToTsqnb(testCase: TestCase): TsqnbFormat {
 // 将换行符替换为 \r\n 格式，以匹配示例文件的格式
  const formattedCode = testCase.code.replace(/\n/g, '\r\n');
  const formattedQuery = testCase.query.replace(/\n/g, '\r\n');
  
  return {
    cells: [
      {
        code: formattedCode,
        language: testCase.language,
        kind: 'code'
      },
      {
        code: formattedQuery,
        language: 'scm',  // Tree-sitter查询语言标识
        kind: 'code'
      }
    ]
  };
}

/**
 * 主函数
 */
function main(): void {
  const args = process.argv.slice(2);

  if (args.length < 2) {
    console.error('Usage: npx ts-node convert-md-to-tsqnb.ts <source-file> <output-directory>');
    console.error('');
    console.error('Example:');
    console.error('  npx ts-node convert-md-to-tsqnb.ts \\');
    console.error('    ./cpp-classes-queries-test-cases.md \\');
    console.error('    ./output/tsqnb');
    process.exit(1);
  }

  const sourceFile = args[0];
  const outputDir = args[1];

  // 验证源文件
 if (!fs.existsSync(sourceFile)) {
    console.error(`Error: Source file not found: ${sourceFile}`);
    process.exit(1);
  }

  try {
    console.log(`Parsing: ${sourceFile}`);

    // 解析Markdown文件
    const testCases = parseMarkdownFile(sourceFile);

    if (testCases.length === 0) {
      console.error('Error: No test cases found in the markdown file');
      process.exit(1);
    }

    console.log(`✓ Found ${testCases.length} test cases`);

    // 创建输出目录
    if (!fs.existsSync(outputDir)) {
      fs.mkdirSync(outputDir, { recursive: true });
    }

    // 为每个测试用例生成tsqnb文件
    testCases.forEach((testCase, index) => {
      const tsqnbData = convertToTsqnb(testCase);
      const outputFileName = `test-${testCase.id}.tsqnb`;
      const outputPath = `${outputDir}/${outputFileName}`;
      
      // 将对象转换为JSON字符串并写入文件
      fs.writeFileSync(outputPath, JSON.stringify(tsqnbData));
      
      console.log(`✓ Generated: ${outputPath}`);
    });

    console.log('✓ Conversion completed successfully!');
  } catch (error) {
    console.error('Error:', error instanceof Error ? error.message : String(error));
    process.exit(1);
  }
}

main();