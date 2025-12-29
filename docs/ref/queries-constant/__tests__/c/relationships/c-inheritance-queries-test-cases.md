# C语言继承关系Tree-Sitter查询规则测试用例

本文档为C语言继承关系的Tree-Sitter查询规则提供测试用例，每个查询规则后都附有相应的测试代码示例，方便在Tree-Sitter Playground中进行验证。

## 1. 结构体嵌套继承关系

### 查询规则
```
(struct_specifier
  name: (type_identifier) @inheritance.parent.struct
  body: (field_declaration_list
    (field_declaration
      type: (struct_specifier
        name: (type_identifier) @inheritance.child.struct)
      declarator: (field_identifier) @inheritance.field.name))) @inheritance.relationship.nested
```

### 测试用例
```c
// 基本结构体嵌套继承
struct Point {
    int x;
    int y;
};

struct Shape {
    struct Point center;  // 嵌套结构体继承
    int color;
};

struct Circle {
    struct Point center;  // 嵌套结构体继承
    int radius;
    int color;
};

// 复杂嵌套继承
struct Address {
    char street[50];
    char city[20];
    char country[20];
};

struct Person {
    char name[50];
    int age;
    struct Address address;  // 嵌套结构体继承
};

struct Employee {
    char name[50];
    int age;
    struct Address address;  // 嵌套结构体继承
    int employee_id;
    float salary;
};

// 多层嵌套继承
struct Dimension {
    int width;
    int height;
    int depth;
};

struct Position {
    struct Dimension size;   // 嵌套结构体继承
    int x;
    int y;
    int z;
};

struct Object3D {
    struct Position pos;     // 嵌套结构体继承
    char name[50];
    int id;
};
```

## 2. 结构体组合关系

### 查询规则
```
(struct_specifier
  name: (type_identifier) @inheritance.container.struct
  body: (field_declaration_list
    (field_declaration
      type: (type_identifier) @inheritance.component.type
      declarator: (field_identifier) @inheritance.component.field))) @inheritance.relationship.composition
```

### 测试用例
```c
// 基本结构体组合
typedef struct Engine Engine;
typedef struct Wheel Wheel;
typedef struct Car Car;

struct Engine {
    int horsepower;
    float displacement;
};

struct Wheel {
    int size;
    char type[20];
};

struct Car {
    Engine engine;      // 组合关系
    Wheel wheels[4];    // 组合关系
    char model[50];
};

// 复杂结构体组合
typedef struct CPU CPU;
typedef struct Memory Memory;
typedef struct Storage Storage;
typedef struct Computer Computer;

struct CPU {
    char model[50];
    int cores;
    float frequency;
};

struct Memory {
    int size_gb;
    char type[20];
    int frequency_mhz;
};

struct Storage {
    int size_gb;
    char type[20];
    int speed_mb_s;
};

struct Computer {
    CPU cpu;            // 组合关系
    Memory memory;      // 组合关系
    Storage storage;    // 组合关系
    char brand[50];
};

// 多层组合关系
typedef struct Keyboard Keyboard;
typedef struct Mouse Mouse;
typedef struct Monitor Monitor;
typedef struct Peripherals Peripherals;

struct Keyboard {
    char layout[20];
    int keys;
    int has_numpad;
};

struct Mouse {
    char type[20];
    int buttons;
    int dpi;
};

struct Monitor {
    int width;
    int height;
    float refresh_rate;
};

struct Peripherals {
    Keyboard keyboard;  // 组合关系
    Mouse mouse;        // 组合关系
    Monitor monitor;    // 组合关系
};

struct Workstation {
    Computer computer;      // 组合关系
    Peripherals peripherals; // 组合关系
    char os[50];
};
```

## 3. 函数指针接口实现关系

### 查询规则
```
(field_declaration
  type: (function_declarator
    declarator: (pointer_declarator
      declarator: (field_identifier) @inheritance.method.pointer)
    parameters: (parameter_list
      (parameter_declaration
        type: (type_identifier) @inheritance.parameter.type
        declarator: (identifier) @inheritance.parameter.name)*)?)
  declarator: (field_identifier) @inheritance.interface.field) @inheritance.relationship.interface
```

### 测试用例
```c
// 基本函数指针接口
typedef struct Drawable Drawable;
typedef struct Point Point;

struct Point {
    int x;
    int y;
};

struct Drawable {
    void (*draw)(Point* position);           // 函数指针接口
    void (*move)(Point* position, int dx, int dy); // 函数指针接口
    int (*get_area)(void);                   // 函数指针接口
};

// 复杂函数指针接口
typedef struct Shape Shape;
typedef struct Rectangle Rectangle;
typedef struct Circle Circle;

struct Shape {
    void (*draw)(void* self);                // 函数指针接口
    void (*resize)(void* self, int width, int height); // 函数指针接口
    int (*get_area)(void* self);             // 函数指针接口
    int (*get_perimeter)(void* self);        // 函数指针接口
};

struct Rectangle {
    Shape base;                              // 继承接口
    int width;
    int height;
};

struct Circle {
    Shape base;                              // 继承接口
    int radius;
};

// 带多个参数的函数指针接口
typedef struct FileHandler FileHandler;
typedef struct Buffer Buffer;

struct Buffer {
    char* data;
    int size;
    int capacity;
};

struct FileHandler {
    int (*open)(const char* filename, const char* mode); // 函数指针接口
    int (*read)(void* buffer, int size, int count);     // 函数指针接口
    int (*write)(const void* buffer, int size, int count); // 函数指针接口
    int (*close)(void);                                 // 函数指针接口
    long (*seek)(long offset, int whence);              // 函数指针接口
};

// 复杂接口实现
typedef struct NetworkInterface NetworkInterface;
typedef struct Socket Socket;

struct NetworkInterface {
    int (*connect)(const char* host, int port);         // 函数指针接口
    int (*send)(const void* data, int size);            // 函数指针接口
    int (*receive)(void* buffer, int size);             // 函数指针接口
    int (*disconnect)(void);                            // 函数指针接口
    int (*get_status)(void);                            // 函数指针接口
};

struct Socket {
    NetworkInterface base;                              // 继承接口
    int socket_fd;
    char remote_host[50];
    int remote_port;
};
```

## 4. 结构体前向声明关系

### 查询规则
```
(struct_specifier
  name: (type_identifier) @inheritance.forward.struct
  body: (field_declaration_list)?) @inheritance.relationship.forward
```

### 测试用例
```c
// 基本前向声明
struct Node;          // 前向声明
struct LinkedList;    // 前向声明

struct Node {
    int data;
    struct Node* next;  // 使用前向声明的结构体
};

struct LinkedList {
    struct Node* head;  // 使用前向声明的结构体
    int count;
};

// 复杂前向声明
struct TreeNode;      // 前向声明
struct BinaryTree;    // 前向声明
struct GraphNode;     // 前向声明
struct Graph;         // 前向声明

struct TreeNode {
    int value;
    struct TreeNode* left;   // 使用前向声明的结构体
    struct TreeNode* right;  // 使用前向声明的结构体
};

struct BinaryTree {
    struct TreeNode* root;   // 使用前向声明的结构体
    int count;
};

struct GraphNode {
    int value;
    struct GraphNode** neighbors;  // 使用前向声明的结构体
    int neighbor_count;
};

struct Graph {
    struct GraphNode* nodes;       // 使用前向声明的结构体
    int node_count;
};

// 相互引用的前向声明
struct Husband;        // 前向声明
struct Wife;           // 前向声明

struct Husband {
    char name[50];
    int age;
    struct Wife* wife;  // 使用前向声明的结构体
};

struct Wife {
    char name[50];
    int age;
    struct Husband* husband;  // 使用前向声明的结构体
};

// 复杂相互引用
struct Company;         // 前向声明
struct Employee;        // 前向声明
struct Department;      // 前向声明

struct Company {
    char name[50];
    struct Department* departments;  // 使用前向声明的结构体
    int department_count;
    struct Employee* ceo;            // 使用前向声明的结构体
};

struct Department {
    char name[50];
    struct Company* company;         // 使用前向声明的结构体
    struct Employee* manager;        // 使用前向声明的结构体
    struct Employee* employees;      // 使用前向声明的结构体
    int employee_count;
};

struct Employee {
    char name[50];
    int age;
    struct Company* company;         // 使用前向声明的结构体
    struct Department* department;   // 使用前向声明的结构体
};
```

## 5. 联合体嵌套关系

### 查询规则
```
(union_specifier
  name: (type_identifier) @inheritance.parent.union
  body: (field_declaration_list
    (field_declaration
      type: (union_specifier
        name: (type_identifier) @inheritance.child.union)
      declarator: (field_identifier) @inheritance.field.name))) @inheritance.relationship.union.nested
```

### 测试用例
```c
// 基本联合体嵌套
union NumericValue {
    int int_value;
    float float_value;
    double double_value;
};

union Data {
    char char_value;
    union NumericValue numeric;  // 嵌套联合体
    char* string_value;
};

// 复杂联合体嵌套
union ColorValue {
    struct {
        unsigned char r, g, b, a;
    } rgba;
    unsigned int color_code;
    char* color_name;
};

union ShapeData {
    struct {
        int x, y;
    } point;
    struct {
        int x, y, width, height;
    } rectangle;
    struct {
        int x, y, radius;
    } circle;
};

union ComplexData {
    char type;
    union ColorValue color;       // 嵌套联合体
    union ShapeData shape;        // 嵌套联合体
    char* text;
};

// 多层联合体嵌套
union PrimitiveValue {
    int int_val;
    float float_val;
    char char_val;
    void* ptr_val;
};

union ArrayValue {
    union PrimitiveValue* values; // 嵌套联合体指针
    int count;
};

union ComplexValue {
    char type;
    union PrimitiveValue primitive; // 嵌套联合体
    union ArrayValue array;         // 嵌套联合体
    char* string;
};
```

## 6. 枚举继承关系（通过枚举值引用）

### 查询规则
```
(enum_specifier
  name: (type_identifier) @inheritance.parent.enum
  body: (enumerator_list
    (enumerator
      name: (identifier) @inheritance.enum.value))) @inheritance.relationship.enum
```

### 测试用例
```c
// 基本枚举定义
enum Color {
    COLOR_RED,
    COLOR_GREEN,
    COLOR_BLUE
};

enum Status {
    STATUS_PENDING,
    STATUS_PROCESSING,
    STATUS_COMPLETED,
    STATUS_FAILED
};

// 扩展枚举（模拟继承）
enum ExtendedColor {
    COLOR_RED,        // 继承基础枚举值
    COLOR_GREEN,      // 继承基础枚举值
    COLOR_BLUE,       // 继承基础枚举值
    COLOR_YELLOW,     // 扩展值
    COLOR_PURPLE,     // 扩展值
    COLOR_ORANGE      // 扩展值
};

enum ExtendedStatus {
    STATUS_PENDING,       // 继承基础枚举值
    STATUS_PROCESSING,    // 继承基础枚举值
    STATUS_COMPLETED,     // 继承基础枚举值
    STATUS_FAILED,        // 继承基础枚举值
    STATUS_CANCELLED,     // 扩展值
    STATUS_SUSPENDED,     // 扩展值
    STATUS_TIMEOUT        // 扩展值
};

// 复杂枚举继承
enum LogLevel {
    LOG_DEBUG,
    LOG_INFO,
    LOG_WARNING,
    LOG_ERROR,
    LOG_CRITICAL
};

enum ExtendedLogLevel {
    LOG_DEBUG,        // 继承基础枚举值
    LOG_INFO,         // 继承基础枚举值
    LOG_WARNING,      // 继承基础枚举值
    LOG_ERROR,        // 继承基础枚举值
    LOG_CRITICAL,     // 继承基础枚举值
    LOG_TRACE,        // 扩展值
    LOG_NOTICE,       // 扩展值
    LOG_ALERT,        // 扩展值
    LOG_EMERGENCY     // 扩展值
};

// 位标志枚举继承
enum FilePermissions {
    PERM_READ = 0x01,
    PERM_WRITE = 0x02,
    PERM_EXECUTE = 0x04
};

enum ExtendedFilePermissions {
    PERM_READ = 0x01,        // 继承基础枚举值
    PERM_WRITE = 0x02,       // 继承基础枚举值
    PERM_EXECUTE = 0x04,     // 继承基础枚举值
    PERM_DELETE = 0x08,      // 扩展值
    PERM_CREATE = 0x10,      // 扩展值
    PERM_MODIFY = 0x20       // 扩展值
};
```

## 7. 类型别名继承关系

### 查询规则
```
(type_definition
  type: (struct_specifier
    name: (type_identifier) @inheritance.base.struct)
  declarator: (type_identifier) @inheritance.derived.type) @inheritance.relationship.type.alias
```

### 测试用例
```c
// 基本类型别名继承
struct Point {
    int x;
    int y;
};

typedef struct Point Point;  // 类型别名继承

struct Rectangle {
    struct Point top_left;
    struct Point bottom_right;
};

typedef struct Rectangle Rectangle;  // 类型别名继承

// 复杂类型别名继承
struct LinkedListNode {
    int data;
    struct LinkedListNode* next;
};

typedef struct LinkedListNode LinkedListNode;  // 类型别名继承

struct LinkedList {
    LinkedListNode* head;
    int count;
};

typedef struct LinkedList LinkedList;  // 类型别名继承

// 嵌套结构体类型别名继承
struct TreeNode {
    int value;
    struct TreeNode* left;
    struct TreeNode* right;
};

typedef struct TreeNode TreeNode;  // 类型别名继承

struct BinaryTree {
    TreeNode* root;
    int count;
};

typedef struct BinaryTree BinaryTree;  // 类型别名继承

// 函数指针类型别名继承
struct Comparator {
    int (*compare)(const void* a, const void* b);
};

typedef struct Comparator Comparator;  // 类型别名继承

struct SortAlgorithm {
    Comparator comparator;
    void (*sort)(void* array, int size, Comparator comp);
};

typedef struct SortAlgorithm SortAlgorithm;  // 类型别名继承

// 复杂嵌套类型别名继承
struct GraphNode {
    int value;
    struct GraphNode** neighbors;
    int neighbor_count;
};

typedef struct GraphNode GraphNode;  // 类型别名继承

struct Graph {
    GraphNode* nodes;
    int node_count;
};

typedef struct Graph Graph;  // 类型别名继承

struct GraphAlgorithm {
    Graph* graph;
    void (*traverse)(Graph* graph, void (*visit)(GraphNode* node));
    int (*find_path)(Graph* graph, GraphNode* start, GraphNode* end, GraphNode** path);
};

typedef struct GraphAlgorithm GraphAlgorithm;  // 类型别名继承
```

## 8. 函数指针数组实现多态关系

### 查询规则
```
(field_declaration
  type: (array_declarator
    type: (pointer_declarator
      type: (function_declarator
        declarator: (field_identifier) @inheritance.polymorphic.method))
    size: (_)?)
  declarator: (field_identifier) @inheritance.vtable.field) @inheritance.relationship.polymorphic
```

### 测试用例
```c
// 基本函数指针数组多态
typedef struct Shape Shape;
typedef struct Rectangle Rectangle;
typedef struct Circle Circle;

struct Shape {
    void (*draw)(Shape* self);
    void (*resize)(Shape* self, int width, int height);
    int (*get_area)(Shape* self);
};

struct Rectangle {
    Shape base;
    int width;
    int height;
};

struct Circle {
    Shape base;
    int radius;
};

// 函数指针数组实现多态
typedef struct Animal Animal;
typedef struct Dog Dog;
typedef struct Cat Cat;

struct Animal {
    void (*make_sound[3])(Animal* self);  // 函数指针数组实现多态
    void (*move[2])(Animal* self, int distance);
    char* name;
};

struct Dog {
    Animal base;
    char breed[50];
};

struct Cat {
    Animal base;
    char color[20];
};

// 复杂函数指针数组多态
typedef struct Vehicle Vehicle;
typedef struct Car Car;
typedef struct Motorcycle Motorcycle;

struct Vehicle {
    void (*start[2])(Vehicle* self);      // 函数指针数组实现多态
    void (*stop[2])(Vehicle* self);
    void (*accelerate[3])(Vehicle* self, int speed);
    void (*brake[2])(Vehicle* self, int force);
    char* model;
};

struct Car {
    Vehicle base;
    int doors;
    char transmission[20];
};

struct Motorcycle {
    Vehicle base;
    int engine_cc;
    char type[20];
};

// 多级函数指针数组多态
typedef struct UIElement UIElement;
typedef struct Button Button;
typedef struct TextBox TextBox;

struct UIElement {
    void (*render[3])(UIElement* self);   // 函数指针数组实现多态
    void (*handle_event[5])(UIElement* self, int event_type);
    void (*update[2])(UIElement* self);
    int x, y, width, height;
};

struct Button {
    UIElement base;
    char text[100];
    void (*click_callback)(Button* self);
};

struct TextBox {
    UIElement base;
    char content[256];
    int max_length;
};
```

## 9. 结构体包含函数指针表关系

### 查询规则
```
(struct_specifier
  name: (type_identifier) @inheritance.class.struct
  body: (field_declaration_list
    (field_declaration
      type: (pointer_declarator
        type: (type_identifier) @inheritance.vtable.type)
      declarator: (field_identifier) @inheritance.vtable.pointer))) @inheritance.relationship.vtable
```

### 测试用例
```c
// 基本虚函数表
typedef struct ShapeVTable ShapeVTable;
typedef struct Shape Shape;
typedef struct Rectangle Rectangle;
typedef struct Circle Circle;

struct ShapeVTable {
    void (*draw)(Shape* self);
    void (*resize)(Shape* self, int width, int height);
    int (*get_area)(Shape* self);
    void (*destroy)(Shape* self);
};

struct Shape {
    ShapeVTable* vtable;  // 虚函数表指针
    int x, y;
};

struct Rectangle {
    Shape base;
    int width;
    int height;
};

struct Circle {
    Shape base;
    int radius;
};

// 复杂虚函数表
typedef struct AnimalVTable AnimalVTable;
typedef struct Animal Animal;
typedef struct Dog Dog;
typedef struct Cat Cat;

struct AnimalVTable {
    void (*make_sound)(Animal* self);
    void (*move)(Animal* self, int distance);
    void (*eat)(Animal* self, char* food);
    void (*sleep)(Animal* self, int hours);
    char* (*get_species)(Animal* self);
};

struct Animal {
    AnimalVTable* vtable;  // 虚函数表指针
    char name[50];
    int age;
};

struct Dog {
    Animal base;
    char breed[50];
};

struct Cat {
    Animal base;
    char color[20];
};

// 多级虚函数表
typedef struct VehicleVTable VehicleVTable;
typedef struct Vehicle Vehicle;
typedef struct Car Car;
typedef struct Motorcycle Motorcycle;

struct VehicleVTable {
    void (*start)(Vehicle* self);
    void (*stop)(Vehicle* self);
    void (*accelerate)(Vehicle* self, int speed);
    void (*brake)(Vehicle* self, int force);
    void (*turn)(Vehicle* self, int direction);
    char* (*get_status)(Vehicle* self);
};

struct Vehicle {
    VehicleVTable* vtable;  // 虚函数表指针
    char model[50];
    int year;
};

struct Car {
    Vehicle base;
    int doors;
    char transmission[20];
};

struct Motorcycle {
    Vehicle base;
    int engine_cc;
    char type[20];
};

// 复杂嵌套虚函数表
typedef struct UIElementVTable UIElementVTable;
typedef struct UIElement UIElement;
typedef struct Button Button;
typedef struct TextBox TextBox;

struct UIElementVTable {
    void (*render)(UIElement* self);
    void (*handle_event)(UIElement* self, int event_type);
    void (*update)(UIElement* self);
    void (*set_focus)(UIElement* self, int focused);
    void (*set_visible)(UIElement* self, int visible);
};

struct UIElement {
    UIElementVTable* vtable;  // 虚函数表指针
    int x, y, width, height;
    int visible;
    int focused;
};

struct Button {
    UIElement base;
    char text[100];
    void (*click_callback)(Button* self);
};

struct TextBox {
    UIElement base;
    char content[256];
    int max_length;
};
```

## 10. 回调函数实现关系

### 查询规则
```
(field_declaration
  type: (function_declarator
    declarator: (pointer_declarator
      declarator: (field_identifier) @inheritance.callback.pointer))
  declarator: (field_identifier) @inheritance.callback.field) @inheritance.relationship.callback
```

### 测试用例
```c
// 基本回调函数
typedef struct Button Button;
typedef struct EventHandler EventHandler;

struct Button {
    char text[100];
    int x, y, width, height;
    void (*on_click)(Button* self);      // 回调函数
    void (*on_hover)(Button* self);      // 回调函数
};

struct EventHandler {
    void (*on_key_press)(int key);      // 回调函数
    void (*on_mouse_click)(int x, int y); // 回调函数
    void (*on_window_resize)(int width, int height); // 回调函数
};

// 复杂回调函数
typedef struct Timer Timer;
typedef struct NetworkClient NetworkClient;

struct Timer {
    int interval_ms;
    int repeat_count;
    void (*on_timeout)(Timer* self);    // 回调函数
    void (*on_tick)(Timer* self);       // 回调函数
    void (*on_complete)(Timer* self);   // 回调函数
};

struct NetworkClient {
    char server[50];
    int port;
    void (*on_connect)(NetworkClient* self);     // 回调函数
    void (*on_disconnect)(NetworkClient* self);  // 回调函数
    void (*on_data_received)(NetworkClient* self, char* data, int size); // 回调函数
    void (*on_error)(NetworkClient* self, int error_code); // 回调函数
};

// 多回调函数结构
typedef struct FileWatcher FileWatcher;

struct FileWatcher {
    char filename[256];
    void (*on_file_changed)(FileWatcher* self);     // 回调函数
    void (*on_file_created)(FileWatcher* self);     // 回调函数
    void (*on_file_deleted)(FileWatcher* self);     // 回调函数
    void (*on_file_moved)(FileWatcher* self, char* new_path); // 回调函数
};

// 异步操作回调
typedef struct AsyncOperation AsyncOperation;

struct AsyncOperation {
    int operation_id;
    void* user_data;
    void (*on_start)(AsyncOperation* self);         // 回调函数
    void (*on_progress)(AsyncOperation* self, int progress); // 回调函数
    void (*on_complete)(AsyncOperation* self, void* result); // 回调函数
    void (*on_error)(AsyncOperation* self, int error_code); // 回调函数
};

// 复杂嵌套回调
typedef struct UIComponent UIComponent;
typedef struct EventManager EventManager;

struct UIComponent {
    int id;
    char name[50];
    void (*on_create)(UIComponent* self);           // 回调函数
    void (*on_destroy)(UIComponent* self);          // 回调函数
    void (*on_update)(UIComponent* self, float delta_time); // 回调函数
    void (*on_render)(UIComponent* self);           // 回调函数
};

struct EventManager {
    UIComponent* components[100];
    int component_count;
    void (*on_component_added)(EventManager* self, UIComponent* component); // 回调函数
    void (*on_component_removed)(EventManager* self, UIComponent* component); // 回调函数
    void (*on_event_fired)(EventManager* self, int event_type, void* event_data); // 回调函数
};
```

## 11. 结构体指针继承关系

### 查询规则
```
(field_declaration
  type: (pointer_declarator
    type: (type_identifier) @inheritance.base.type)
  declarator: (field_identifier) @inheritance.pointer.field)) @inheritance.relationship.pointer
```

### 测试用例
```c
// 基本结构体指针继承
typedef struct Base Base;
typedef struct Derived Derived;

struct Base {
    int base_value;
    char base_name[50];
};

struct Derived {
    Base* base;        // 结构体指针继承
    int derived_value;
    char derived_name[50];
};

// 复杂结构体指针继承
typedef struct Animal Animal;
typedef struct Mammal Mammal;
typedef struct Dog Dog;

struct Animal {
    char name[50];
    int age;
    void (*make_sound)(void);
};

struct Mammal {
    Animal* animal;    // 结构体指针继承
    int fur_length;
    char species[50];
};

struct Dog {
    Mammal* mammal;    // 结构体指针继承
    char breed[50];
    int tail_length;
};

// 多级结构体指针继承
typedef struct Vehicle Vehicle;
typedef struct Car Car;
typedef struct SportsCar SportsCar;

struct Vehicle {
    char model[50];
    int year;
    int speed;
};

struct Car {
    Vehicle* vehicle;  // 结构体指针继承
    int doors;
    char transmission[20];
};

struct SportsCar {
    Car* car;          // 结构体指针继承
    int turbo_boost;
    char aerodynamics[50];
};

// 复杂嵌套指针继承
typedef struct Component Component;
typedef struct UIComponent UIComponent;
typedef struct Button Button;

struct Component {
    int id;
    char name[50];
    void (*update)(void);
};

struct UIComponent {
    Component* component;  // 结构体指针继承
    int x, y, width, height;
    int visible;
};

struct Button {
    UIComponent* ui_component;  // 结构体指针继承
    char text[100];
    void (*on_click)(void);
};

// 多重指针继承
typedef struct Node Node;
typedef struct TreeNode TreeNode;
typedef struct BinaryTreeNode BinaryTreeNode;

struct Node {
    int data;
    Node* next;
};

struct TreeNode {
    Node* node;         // 结构体指针继承
    TreeNode** children;
    int child_count;
};

struct BinaryTreeNode {
    TreeNode* tree_node;  // 结构体指针继承
    BinaryTreeNode* left;
    BinaryTreeNode* right;
};
```

## 12. 嵌套结构体访问关系

### 查询规则
```
(field_expression
  argument: (identifier) @inheritance.outer.struct
  field: (field_identifier) @inheritance.inner.field)) @inheritance.relationship.access
```

### 测试用例
```c
// 基本嵌套结构体访问
struct Point {
    int x;
    int y;
};

struct Rectangle {
    struct Point top_left;
    struct Point bottom_right;
};

void access_nested_struct() {
    struct Rectangle rect;
    rect.top_left.x = 10;      // 嵌套结构体访问
    rect.top_left.y = 20;      // 嵌套结构体访问
    rect.bottom_right.x = 30;  // 嵌套结构体访问
    rect.bottom_right.y = 40;  // 嵌套结构体访问
}

// 复杂嵌套结构体访问
struct Address {
    char street[50];
    char city[20];
    char country[20];
};

struct Person {
    char name[50];
    int age;
    struct Address address;
};

struct Employee {
    struct Person person;
    int employee_id;
    float salary;
};

void access_complex_nested() {
    struct Employee emp;
    emp.person.age = 30;                    // 嵌套结构体访问
    emp.person.address.city[0] = 'N';       // 嵌套结构体访问
    emp.person.address.city[1] = 'e';       // 嵌套结构体访问
    emp.person.address.city[2] = 'w';       // 嵌套结构体访问
    emp.person.address.city[3] = '\0';      // 嵌套结构体访问
}

// 多级嵌套结构体访问
struct Dimension {
    int width;
    int height;
    int depth;
};

struct Position {
    struct Dimension size;
    int x, y, z;
};

struct Object3D {
    struct Position position;
    char name[50];
    int id;
};

void access_multilevel_nested() {
    struct Object3D obj;
    obj.position.size.width = 100;   // 多级嵌套结构体访问
    obj.position.size.height = 200;  // 多级嵌套结构体访问
    obj.position.size.depth = 50;    // 多级嵌套结构体访问
    obj.position.x = 10;             // 嵌套结构体访问
    obj.position.y = 20;             // 嵌套结构体访问
    obj.position.z = 30;             // 嵌套结构体访问
}

// 指针嵌套结构体访问
struct Node {
    int data;
    struct Node* next;
};

struct LinkedList {
    struct Node* head;
    int count;
};

void access_pointer_nested() {
    struct LinkedList list;
    list.head->data = 10;        // 指针嵌套结构体访问
    list.head->next->data = 20;  // 多级指针嵌套结构体访问
}

// 函数参数中的嵌套结构体访问
void process_rectangle(struct Rectangle* rect) {
    rect->top_left.x = 0;        // 指针嵌套结构体访问
    rect->top_left.y = 0;        // 指针嵌套结构体访问
    rect->bottom_right.x = 100;  // 指针嵌套结构体访问
    rect->bottom_right.y = 100;  // 指针嵌套结构体访问
}

void process_employee(struct Employee* emp) {
    emp->person.age = 25;                    // 指针嵌套结构体访问
    emp->person.address.country[0] = 'U';    // 指针嵌套结构体访问
    emp->person.address.country[1] = 'S';    // 指针嵌套结构体访问
    emp->person.address.country[2] = 'A';    // 指针嵌套结构体访问
    emp->person.address.country[3] = '\0';   // 指针嵌套结构体访问
}
```

## 13. 综合测试用例

### 测试用例
```c
// 综合继承关系示例
#include <stdio.h>
#include <stdlib.h>

// 前向声明
struct ShapeVTable;
struct Shape;
struct Rectangle;
struct Circle;

// 虚函数表定义
struct ShapeVTable {
    void (*draw)(struct Shape* self);
    void (*resize)(struct Shape* self, int width, int height);
    int (*get_area)(struct Shape* self);
    void (*destroy)(struct Shape* self);
};

// 基础结构体
struct Point {
    int x;
    int y;
};

// 基础形状结构体
struct Shape {
    struct ShapeVTable* vtable;  // 虚函数表指针
    struct Point position;       // 嵌套结构体继承
    char color[20];
};

// 派生结构体
struct Rectangle {
    struct Shape base;           // 继承基础结构体
    int width;
    int height;
};

struct Circle {
    struct Shape base;           // 继承基础结构体
    int radius;
};

// 类型别名
typedef struct Shape Shape;
typedef struct Rectangle Rectangle;
typedef struct Circle Circle;
typedef struct Point Point;

// 回调函数结构体
struct ShapeEventHandler {
    void (*on_shape_created)(Shape* shape);     // 回调函数
    void (*on_shape_destroyed)(Shape* shape);    // 回调函数
    void (*on_shape_resized)(Shape* shape);      // 回调函数
};

// 复杂组合结构体
struct ShapeManager {
    Shape* shapes[100];          // 形状数组
    int shape_count;
    struct ShapeEventHandler* event_handler;  // 事件处理器
    Point origin;                // 原点
};

// 函数实现
void rectangle_draw(Shape* self) {
    Rectangle* rect = (Rectangle*)self;
    printf("Drawing rectangle at (%d, %d) with size %dx%d\n", 
           self->position.x, self->position.y, 
           rect->width, rect->height);
}

void rectangle_resize(Shape* self, int width, int height) {
    Rectangle* rect = (Rectangle*)self;
    rect->width = width;
    rect->height = height;
}

int rectangle_get_area(Shape* self) {
    Rectangle* rect = (Rectangle*)self;
    return rect->width * rect->height;
}

void circle_draw(Shape* self) {
    Circle* circle = (Circle*)self;
    printf("Drawing circle at (%d, %d) with radius %d\n", 
           self->position.x, self->position.y, 
           circle->radius);
}

void circle_resize(Shape* self, int width, int height) {
    Circle* circle = (Circle*)self;
    // 使用较小的值作为半径
    circle->radius = (width < height) ? width / 2 : height / 2;
}

int circle_get_area(Shape* self) {
    Circle* circle = (Circle*)self;
    return 3.14159 * circle->radius * circle->radius;
}

// 虚函数表实例
struct ShapeVTable rectangle_vtable = {
    rectangle_draw,
    rectangle_resize,
    rectangle_get_area,
    NULL
};

struct ShapeVTable circle_vtable = {
    circle_draw,
    circle_resize,
    circle_get_area,
    NULL
};

// 创建函数
Rectangle* create_rectangle(int x, int y, int width, int height) {
    Rectangle* rect = malloc(sizeof(Rectangle));
    rect->base.vtable = &rectangle_vtable;
    rect->base.position.x = x;      // 嵌套结构体访问
    rect->base.position.y = y;      // 嵌套结构体访问
    rect->width = width;
    rect->height = height;
    return rect;
}

Circle* create_circle(int x, int y, int radius) {
    Circle* circle = malloc(sizeof(Circle));
    circle->base.vtable = &circle_vtable;
    circle->base.position.x = x;    // 嵌套结构体访问
    circle->base.position.y = y;    // 嵌套结构体访问
    circle->radius = radius;
    return circle;
}

// 使用示例
void demonstrate_inheritance() {
    // 创建形状
    Shape* shapes[2];
    shapes[0] = (Shape*)create_rectangle(10, 20, 100, 50);
    shapes[1] = (Shape*)create_circle(30, 40, 25);
    
    // 多态调用
    for (int i = 0; i < 2; i++) {
        shapes[i]->vtable->draw(shapes[i]);           // 虚函数表调用
        printf("Area: %d\n", shapes[i]->vtable->get_area(shapes[i])); // 虚函数表调用
        shapes[i]->vtable->resize(shapes[i], 200, 100); // 虚函数表调用
        shapes[i]->vtable->draw(shapes[i]);           // 虚函数表调用
    }
    
    // 嵌套结构体访问
    Rectangle* rect = (Rectangle*)shapes[0];
    printf("Rectangle position: (%d, %d)\n", 
           rect->base.position.x, rect->base.position.y); // 多级嵌套访问
    
    Circle* circle = (Circle*)shapes[1];
    printf("Circle position: (%d, %d)\n", 
           circle->base.position.x, circle->base.position.y); // 多级嵌套访问
    
    // 清理
    free(shapes[0]);
    free(shapes[1]);
}