#!/usr/bin/env python3
import json
import re

def extract_and_parse_json(input_file, output_file):
    # 读取文件内容
    with open(input_file, 'r', encoding='utf-8') as f:
        content = f.read()
    
    # 查找 = " 后面的内容
    match = re.search(r'=\s*"(\[.*\])"\s*;', content, re.DOTALL)
    
    if not match:
        print("未找到匹配的JSON")
        match = re.search(r'=\s*r"(\[.*\])"\s*;', content, re.DOTALL)
    
    if not match:
        print("尝试手动提取...")
        start_quote = content.find('"[')
        if start_quote == -1:
            print("无法找到JSON开始")
            return
        end_quote = -1
        i = start_quote + 2
        brace_count = 0
        in_string = False
        escape = False
        
        while i < len(content):
            char = content[i]
            
            if escape:
                escape = False
            elif char == '\\':
                escape = True
            elif char == '"' and not in_string:
                if brace_count == 0:
                    end_quote = i
                    break
            elif char == '"' and in_string:
                in_string = False
            elif char == '"' and not in_string:
                in_string = True
            elif char == '[' and not in_string:
                brace_count += 1
            elif char == ']' and not in_string:
                brace_count -= 1
            
            i += 1
        
        if end_quote == -1:
            print("无法找到JSON结束")
            return
        
        json_str = content[start_quote+1:end_quote]
    else:
        json_str = match.group(1)
    
    # 处理转义字符
    json_str = json_str.replace('\\"', '"')
    json_str = json_str.replace('\\n', '\n')
    json_str = json_str.replace('\\t', '\t')
    json_str = json_str.replace('\\\\', '\\')
    
    # 尝试解析
    try:
        data = json.loads(json_str)
        print(f"成功解析，共 {len(data)} 个节点")
    except json.JSONDecodeError as e:
        print(f"JSON解析错误: {e}")
        print(f"错误位置: {e.pos}")
        print(f"错误位置附近内容:")
        start = max(0, e.pos - 100)
        end = min(len(json_str), e.pos + 100)
        print(repr(json_str[start:end]))
        return
    
    # 构建父子关系映射
    child_to_parents = {}
    
    for node in data:
        node_type = node.get('type')
        if not node_type:
            continue
        
        # 从subtypes建立关系
        for subtype in node.get('subtypes', []):
            if isinstance(subtype, dict):
                child_type = subtype.get('type')
                if child_type:
                    if child_type not in child_to_parents:
                        child_to_parents[child_type] = []
                    if node_type not in child_to_parents[child_type]:
                        child_to_parents[child_type].append(node_type)
        
        # 从fields建立关系 - 修复类型检查
        fields = node.get('fields', [])
        for field in fields:
            # 检查field是字典还是字符串
            if isinstance(field, dict):
                field_types = field.get('types', [])
                for field_type in field_types:
                    if isinstance(field_type, dict):
                        child_type = field_type.get('type')
                        if child_type:
                            if child_type not in child_to_parents:
                                child_to_parents[child_type] = []
                            if node_type not in child_to_parents[child_type]:
                                child_to_parents[child_type].append(node_type)
    
    # 写入输出文件
    with open(output_file, 'w', encoding='utf-8') as f:
        # 写入每个节点的信息
        for node in data:
            node_type = node.get('type', 'unknown')
            named = node.get('named', False)
            
            f.write(f"Node: {node_type}\n")
            f.write(f"  Named: {named}\n")
            
            # 获取父节点
            parents = child_to_parents.get(node_type, [])
            if parents:
                f.write(f"  Parents: {', '.join(parents)}\n")
            else:
                f.write(f"  Parents: (root)\n")
            
            # 处理 subtypes - 直接子节点
            subtypes = node.get('subtypes', [])
            if subtypes:
                subtype_names = []
                for s in subtypes:
                    if isinstance(s, dict):
                        subtype_names.append(s.get('type', 'unknown'))
                if subtype_names:
                    f.write(f"  Direct Children: {', '.join(subtype_names)}\n")
            
            # 处理 fields
            fields = node.get('fields', [])
            if fields:
                field_info = []
                for field in fields:
                    if isinstance(field, dict):
                        field_name = field.get('name', 'unknown')
                        field_types = []
                        for t in field.get('types', []):
                            if isinstance(t, dict):
                                field_types.append(t.get('type', 'unknown'))
                        if field_types:
                            field_info.append(f"{field_name}: {', '.join(field_types)}")
                if field_info:
                    f.write(f"  Fields: {'; '.join(field_info)}\n")
            
            f.write("\n")
    
    print(f"结果已保存到 {output_file}")
    print(f"总节点数: {len(data)}")
    print(f"有父子关系的节点数: {len(child_to_parents)}")

if __name__ == "__main__":
    import sys
    
    input_file = sys.argv[1] if len(sys.argv) > 1 else "bash-node-raw.txt"
    output_file = sys.argv[2] if len(sys.argv) > 2 else "output.txt"
    
    print(f"输入文件: {input_file}")
    print(f"输出文件: {output_file}")
    
    extract_and_parse_json(input_file, output_file)