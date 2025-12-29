# C语言创建关系Tree-Sitter查询规则测试用例

本文档为C语言创建关系的Tree-Sitter查询规则提供测试用例，每个查询规则后都附有相应的测试代码示例，方便在Tree-Sitter Playground中进行验证。

## 1. 变量声明创建关系

### 查询规则
```
(declaration
  type: (type_identifier) @creation.type
  declarator: (identifier) @creation.variable) @creation.relationship.variable
```

### 测试用例
```c
// 基本变量声明
int integer_variable;
char character_variable;
float floating_variable;
double double_variable;

// 自定义类型变量声明
typedef struct Point Point;
Point point_variable;

typedef enum Color Color;
Color color_variable;

// 复杂类型变量声明
typedef struct ComplexType ComplexType;
ComplexType complex_variable;

// 带修饰符的变量声明
const int constant_variable;
static int static_variable;
extern int external_variable;
volatile int volatile_variable;

// 指针类型变量声明
typedef int* IntPtr;
IntPtr pointer_variable;

// 函数指针类型声明
typedef void (*FunctionPtr)(int);
FunctionPtr function_pointer_variable;
```

## 2. 结构体实例化关系

### 查询规则
```
(declaration
  type: (struct_specifier
    name: (type_identifier) @creation.struct.type)
  declarator: (identifier) @creation.struct.instance) @creation.relationship.struct
```

### 测试用例
```c
// 基本结构体实例化
struct Point {
    int x;
    int y;
};
struct Point point1;
struct Point point2;

// 嵌套结构体实例化
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
struct Person person1;
struct Person person2;

// 复杂结构体实例化
struct TreeNode {
    int value;
    struct TreeNode* left;
    struct TreeNode* right;
};
struct TreeNode tree_node;

// 带函数指针的结构体实例化
struct Handler {
    void (*init)(void);
    void (*process)(int);
    void (*cleanup)(void);
};
struct Handler handler;

// 匿名结构体实例化
struct {
    int x;
    int y;
} anonymous_point;

// 自引用结构体实例化
struct LinkedList {
    int data;
    struct LinkedList* next;
};
struct LinkedList list_node;
```

## 3. 数组创建关系

### 查询规则
```
(declaration
  type: (array_declarator
    type: (_) @creation.array.type
    size: (_) @creation.array.size)
  declarator: (identifier) @creation.array.name) @creation.relationship.array
```

### 测试用例
```c
// 基本数组创建
int int_array[10];
char char_array[256];
float float_array[100];
double double_array[50];

// 多维数组创建
int matrix[3][3];
int cube[3][3][3];
char string_array[10][50];

// 结构体数组创建
struct Point {
    int x;
    int y;
};
struct Point points[100];

// 指针数组创建
int* pointer_array[10];
void* void_pointer_array[20];

// 函数指针数组创建
typedef int (*OperationFunc)(int, int);
OperationFunc operations[4];

// 带变量大小的数组创建
int size = 10;
int variable_array[size];  // VLA (Variable Length Array)

// 复杂数组创建
struct {
    int id;
    char name[50];
} complex_array[100];

// 常量表达式大小的数组创建
#define ARRAY_SIZE 100
int constant_array[ARRAY_SIZE];
int expression_array[10 * 5 + 2];
```

## 4. 指针创建关系

### 查询规则
```
(declaration
  type: (pointer_declarator
    type: (_) @creation.pointer.type)
  declarator: (identifier) @creation.pointer.name) @creation.relationship.pointer
```

### 测试用例
```c
// 基本指针创建
int* int_pointer;
char* char_pointer;
float* float_pointer;
double* double_pointer;

// 结构体指针创建
struct Point {
    int x;
    int y;
};
struct Point* point_pointer;

// 多级指针创建
int** double_pointer;
int*** triple_pointer;

// 函数指针创建
typedef void (*FunctionPtr)(int);
FunctionPtr function_pointer;

// 常量指针创建
const int* const_int_pointer;
int* const int_const_pointer;
const int* const const_int_const_pointer;

// void指针创建
void* void_pointer;

// 复杂指针创建
struct ComplexStruct {
    int data;
    struct ComplexStruct* next;
};
struct ComplexStruct* complex_pointer;

// 指针数组指针创建
int (*array_pointer)[10];

// 函数指针数组指针创建
typedef int (*OperationFunc)(int, int);
OperationFunc (*func_array_pointer)[4];
```

## 5. 内存分配创建关系

### 查询规则
```
(init_declarator
  declarator: (pointer_declarator
    declarator: (identifier) @creation.variable)
  value: (call_expression
    function: (identifier) @allocation.function
    (#match? @allocation.function "^(malloc|calloc|realloc)$")
    arguments: (argument_list
      (_) @allocation.size))) @creation.relationship.allocation
```

### 测试用例
```c
#include <stdlib.h>

// malloc内存分配
int* int_pointer = malloc(sizeof(int) * 10);
char* char_pointer = malloc(sizeof(char) * 256);
float* float_pointer = malloc(sizeof(float) * 100);
double* double_pointer = malloc(sizeof(double) * 50);

// calloc内存分配
int* int_array = calloc(10, sizeof(int));
char* char_array = calloc(256, sizeof(char));
float* float_array = calloc(100, sizeof(float));

// realloc内存分配
int* resized_int_array = realloc(int_pointer, sizeof(int) * 20);
char* resized_char_array = realloc(char_pointer, sizeof(char) * 512);

// 结构体内存分配
struct Point {
    int x;
    int y;
};
struct Point* points = malloc(sizeof(struct Point) * 100);

// 复杂结构体内存分配
struct ComplexStruct {
    int id;
    char name[50];
    struct ComplexStruct* next;
};
struct ComplexStruct* complex_array = calloc(10, sizeof(struct ComplexStruct));

// 函数指针内存分配
typedef void (*FunctionPtr)(int);
FunctionPtr* function_array = malloc(sizeof(FunctionPtr) * 5);

// 多级指针内存分配
int** double_pointer = malloc(sizeof(int*) * 10);
for (int i = 0; i < 10; i++) {
    double_pointer[i] = malloc(sizeof(int) * 5);
}

// void指针内存分配
void* void_pointer = malloc(1024);

// 带表达式大小的内存分配
int size = 100;
int* dynamic_array = malloc(sizeof(int) * size);
int* dynamic_calloc = calloc(size, sizeof(int));
```

## 6. 文件创建关系

### 查询规则
```
(init_declarator
  declarator: (identifier) @creation.file.handle
  value: (call_expression
    function: (identifier) @file.open.function
    (#match? @file.open.function "^(fopen|open|create)$")
    arguments: (argument_list
      (string_literal) @file.path
      (_) @file.mode))) @creation.relationship.file
```

### 测试用例
```c
#include <stdio.h>
#include <fcntl.h>
#include <sys/stat.h>
#include <unistd.h>

// fopen文件创建
FILE* text_file = fopen("example.txt", "w");
FILE* binary_file = fopen("data.bin", "wb");
FILE* read_file = fopen("input.txt", "r");
FILE* append_file = fopen("log.txt", "a");
FILE* read_write_file = fopen("config.ini", "r+");

// 带路径的fopen文件创建
FILE* absolute_path_file = fopen("/tmp/temp.txt", "w");
FILE* relative_path_file = fopen("../data/input.dat", "rb");
FILE* current_dir_file = fopen("./output.log", "a");

// open文件创建 (POSIX)
int file_descriptor = open("data.txt", O_CREAT | O_WRONLY, 0644);
int read_only_fd = open("input.txt", O_RDONLY);
int write_only_fd = open("output.txt", O_WRONLY);
int read_write_fd = open("config.txt", O_RDWR);

// 带标志的open文件创建
int exclusive_create = open("unique.txt", O_CREAT | O_EXCL | O_WRONLY, 0644);
int append_only = open("log.txt", O_CREAT | O_APPEND | O_WRONLY, 0644);
int truncate_file = open("data.txt", O_CREAT | O_TRUNC | O_WRONLY, 0644);

// create文件创建 (POSIX)
int new_file = create("newfile.txt", 0644);

// 带复杂路径的文件创建
FILE* complex_path_file = fopen("/var/log/application.log", "a");
int complex_fd = open("/home/user/documents/report.pdf", O_CREAT | O_RDONLY, 0644);

// 带变量路径的文件创建
char* filename = "dynamic.txt";
FILE* dynamic_file = fopen(filename, "w");

// 带变量模式的文件创建
char* mode = "rb";
FILE* dynamic_mode_file = fopen("data.bin", mode);
```

## 7. 线程创建关系

### 查询规则
```
(call_expression
  function: (identifier) @thread.create.function
  (#match? @thread.create.function "^(pthread_create|CreateThread|_beginthread)$")
  arguments: (argument_list
    (identifier) @thread.handle
    (_) @thread.attributes
    (identifier) @thread.function
    (_) @thread.argument)) @creation.relationship.thread
```

### 测试用例
```c
#include <pthread.h>
#include <windows.h>
#include <process.h>

// pthread_create线程创建
pthread_t thread1;
pthread_attr_t attr1;
void* thread_function1(void* arg);
pthread_create(&thread1, &attr1, thread_function1, NULL);

pthread_t thread2;
void* thread_function2(void* arg);
int thread_arg = 42;
pthread_create(&thread2, NULL, thread_function2, &thread_arg);

// CreateThread线程创建 (Windows)
HANDLE win_thread1;
DWORD thread_id1;
LPTHREAD_START_ROUTINE start_function1;
LPVOID thread_param1;
HANDLE win_thread1 = CreateThread(NULL, 0, start_function1, thread_param1, 0, &thread_id1);

HANDLE win_thread2;
DWORD thread_id2;
HANDLE win_thread2 = CreateThread(NULL, 4096, start_function1, thread_param1, CREATE_SUSPENDED, &thread_id2);

// _beginthread线程创建 (Windows)
uintptr_t begin_thread1;
void thread_func1(void* arg);
uintptr_t begin_thread1 = _beginthread(thread_func1, 0, NULL);

uintptr_t begin_thread2;
void thread_func2(void* arg);
int param2 = 100;
uintptr_t begin_thread2 = _beginthread(thread_func1, 4096, &param2);

// 复杂线程创建示例
pthread_t worker_threads[5];
pthread_attr_t worker_attr;
void* worker_function(void* arg);
int worker_args[5] = {1, 2, 3, 4, 5};

for (int i = 0; i < 5; i++) {
    pthread_create(&worker_threads[i], &worker_attr, worker_function, &worker_args[i]);
}

// 带结构体参数的线程创建
struct ThreadData {
    int id;
    char name[50];
    void* context;
};
pthread_t data_thread;
struct ThreadData thread_data = {1, "Worker Thread", NULL};
void* data_thread_function(void* arg);
pthread_create(&data_thread, NULL, data_thread_function, &thread_data);
```

## 8. 互斥锁创建关系

### 查询规则
```
(call_expression
  function: (identifier) @mutex.create.function
  (#match? @mutex.create.function "^(pthread_mutex_init|InitializeCriticalSection)$")
  arguments: (argument_list
    (pointer_expression
      argument: (identifier) @mutex.handle)
    (_) @mutex.attributes)) @creation.relationship.mutex
```

### 测试用例
```c
#include <pthread.h>
#include <windows.h>

// pthread_mutex_init互斥锁创建
pthread_mutex_t mutex1;
pthread_mutexattr_t attr1;
pthread_mutex_init(&mutex1, &attr1);

pthread_mutex_t mutex2;
pthread_mutex_init(&mutex2, NULL);

// 带属性的互斥锁创建
pthread_mutex_t recursive_mutex;
pthread_mutexattr_t recursive_attr;
pthread_mutexattr_init(&recursive_attr);
pthread_mutexattr_settype(&recursive_attr, PTHREAD_MUTEX_RECURSIVE);
pthread_mutex_init(&recursive_mutex, &recursive_attr);

// InitializeCriticalSection互斥锁创建 (Windows)
CRITICAL_SECTION critical_section1;
InitializeCriticalSection(&critical_section1);

// InitializeCriticalSectionAndSpinCount互斥锁创建 (Windows)
CRITICAL_SECTION critical_section2;
InitializeCriticalSectionAndSpinCount(&critical_section2, 4000);

// 复杂互斥锁创建示例
pthread_mutex_t shared_mutexes[10];
pthread_mutexattr_t shared_attr;
pthread_mutexattr_init(&shared_attr);
pthread_mutexattr_setpshared(&shared_attr, PTHREAD_PROCESS_SHARED);

for (int i = 0; i < 10; i++) {
    pthread_mutex_init(&shared_mutexes[i], &shared_attr);
}

// 带结构体的互斥锁创建
struct MutexWrapper {
    pthread_mutex_t mutex;
    int counter;
    char name[50];
};
struct MutexWrapper wrapper;
pthread_mutexattr_t wrapper_attr;
pthread_mutex_init(&wrapper.mutex, &wrapper_attr);
```

## 9. 条件变量创建关系

### 查询规则
```
(call_expression
  function: (identifier) @condition.create.function
  (#match? @condition.create.function "^(pthread_cond_init)$")
  arguments: (argument_list
    (pointer_expression
      argument: (identifier) @condition.handle)
    (_) @condition.attributes)) @creation.relationship.condition
```

### 测试用例
```c
#include <pthread.h>

// pthread_cond_init条件变量创建
pthread_cond_t condition1;
pthread_condattr_t attr1;
pthread_cond_init(&condition1, &attr1);

pthread_cond_t condition2;
pthread_cond_init(&condition2, NULL);

// 带共享属性的条件变量创建
pthread_cond_t shared_condition;
pthread_condattr_t shared_attr;
pthread_condattr_init(&shared_attr);
pthread_condattr_setpshared(&shared_attr, PTHREAD_PROCESS_SHARED);
pthread_cond_init(&shared_condition, &shared_attr);

// 复杂条件变量创建示例
pthread_cond_t producer_conditions[5];
pthread_cond_t consumer_conditions[5];
pthread_condattr_t cond_attr;

pthread_condattr_init(&cond_attr);
pthread_condattr_setpshared(&cond_attr, PTHREAD_PROCESS_SHARED);

for (int i = 0; i < 5; i++) {
    pthread_cond_init(&producer_conditions[i], &cond_attr);
    pthread_cond_init(&consumer_conditions[i], &cond_attr);
}

// 带结构体的条件变量创建
struct ConditionWrapper {
    pthread_cond_t condition;
    pthread_mutex_t mutex;
    int signal_count;
};
struct ConditionWrapper wrapper;
pthread_condattr_t wrapper_attr;
pthread_mutexattr_t mutex_attr;

pthread_cond_init(&wrapper.condition, &wrapper_attr);
pthread_mutex_init(&wrapper.mutex, &mutex_attr);

// 带时钟属性的条件变量创建
pthread_cond_t timed_condition;
pthread_condattr_t timed_attr;
pthread_condattr_init(&timed_attr);
pthread_condattr_setclock(&timed_attr, CLOCK_MONOTONIC);
pthread_cond_init(&timed_condition, &timed_attr);
```

## 10. 信号量创建关系

### 查询规则
```
(call_expression
  function: (identifier) @semaphore.create.function
  (#match? @semaphore.create.function "^(sem_init|sem_open)$")
  arguments: (argument_list
    (pointer_expression
      argument: (identifier) @semaphore.handle)
    (_) @semaphore.pshared
    (_) @semaphore.value)) @creation.relationship.semaphore
```

### 测试用例
```c
#include <semaphore.h>
#include <fcntl.h>

// sem_init信号量创建
sem_t semaphore1;
sem_init(&semaphore1, 0, 1);

sem_t semaphore2;
sem_init(&semaphore2, 1, 5);  // 进程间共享信号量

// 带初始值的信号量创建
sem_t counting_semaphore;
sem_init(&counting_semaphore, 0, 10);

// 二进制信号量创建
sem_t binary_semaphore;
sem_init(&binary_semaphore, 0, 1);

// sem_open命名信号量创建
sem_t* named_semaphore1;
named_semaphore1 = sem_open("/my_semaphore", O_CREAT, 0644, 1);

sem_t* named_semaphore2;
named_semaphore2 = sem_open("/shared_counter", O_CREAT | O_EXCL, 0644, 0);

// 复杂信号量创建示例
sem_t worker_semaphores[5];
for (int i = 0; i < 5; i++) {
    sem_init(&worker_semaphores[i], 0, 0);
}

// 带结构体的信号量创建
struct SemaphoreWrapper {
    sem_t semaphore;
    int max_count;
    char name[50];
};
struct SemaphoreWrapper wrapper;
sem_init(&wrapper.semaphore, 0, wrapper.max_count);

// 带变量参数的信号量创建
int initial_value = 5;
int shared_flag = 1;
sem_t dynamic_semaphore;
sem_init(&dynamic_semaphore, shared_flag, initial_value);

// 带标志的sem_open信号量创建
sem_t* exclusive_semaphore;
exclusive_semaphore = sem_open("/exclusive_sem", O_CREAT | O_EXCL, 0644, 1);
```

## 11. 套接字创建关系

### 查询规则
```
(call_expression
  function: (identifier) @socket.create.function
  (#match? @socket.create.function "^(socket|WSASocket)$")
  arguments: (argument_list
    (_) @socket.domain
    (_) @socket.type
    (_) @socket.protocol)) @creation.relationship.socket
```

### 测试用例
```c
#include <sys/socket.h>
#include <winsock2.h>

// socket套接字创建
int tcp_socket = socket(AF_INET, SOCK_STREAM, 0);
int udp_socket = socket(AF_INET, SOCK_DGRAM, 0);
int raw_socket = socket(AF_INET, SOCK_RAW, IPPROTO_RAW);

// IPv6套接字创建
int ipv6_tcp_socket = socket(AF_INET6, SOCK_STREAM, 0);
int ipv6_udp_socket = socket(AF_INET6, SOCK_DGRAM, 0);

// Unix域套接字创建
int unix_stream_socket = socket(AF_UNIX, SOCK_STREAM, 0);
int unix_dgram_socket = socket(AF_UNIX, SOCK_DGRAM, 0);

// 带特定协议的套接字创建
int icmp_socket = socket(AF_INET, SOCK_RAW, IPPROTO_ICMP);
int tcp_socket_specific = socket(AF_INET, SOCK_STREAM, IPPROTO_TCP);
int udp_socket_specific = socket(AF_INET, SOCK_DGRAM, IPPROTO_UDP);

// WSASocket套接字创建 (Windows)
SOCKET win_socket = WSASocket(AF_INET, SOCK_STREAM, IPPROTO_TCP, NULL, 0, 0);
SOCKET win_udp_socket = WSASocket(AF_INET, SOCK_DGRAM, IPPROTO_UDP, NULL, 0, 0);

// 带重叠I/O的WSASocket创建
SOCKET overlapped_socket = WSASocket(AF_INET, SOCK_STREAM, IPPROTO_TCP, NULL, 0, WSA_FLAG_OVERLAPPED);

// 复杂套接字创建示例
int server_sockets[10];
for (int i = 0; i < 10; i++) {
    server_sockets[i] = socket(AF_INET, SOCK_STREAM, 0);
}

// 带变量参数的套接字创建
int domain = AF_INET;
int type = SOCK_STREAM;
int protocol = IPPROTO_TCP;
int dynamic_socket = socket(domain, type, protocol);

// 带结构体的套接字创建
struct SocketWrapper {
    int socket_fd;
    int domain;
    int type;
    int protocol;
};
struct SocketWrapper wrapper;
wrapper.socket_fd = socket(wrapper.domain, wrapper.type, wrapper.protocol);
```

## 12. 综合测试用例

### 测试用例
```c
#include <stdio.h>
#include <stdlib.h>
#include <pthread.h>
#include <semaphore.h>
#include <sys/socket.h>
#include <fcntl.h>

// 综合创建关系示例
void comprehensive_creation_example() {
    // 变量声明创建
    int counter = 0;
    char message[256];
    
    // 结构体实例化创建
    struct Point {
        int x;
        int y;
    } point = {10, 20};
    
    // 数组创建
    int numbers[100];
    struct Point points[50];
    
    // 指针创建
    int* int_ptr;
    struct Point* point_ptr;
    
    // 内存分配创建
    int* dynamic_array = malloc(sizeof(int) * 1000);
    struct Point* dynamic_points = calloc(50, sizeof(struct Point));
    
    // 文件创建
    FILE* log_file = fopen("application.log", "a");
    FILE* data_file = fopen("data.bin", "wb");
    
    // 线程创建
    pthread_t worker_thread;
    void* worker_function(void* arg);
    pthread_create(&worker_thread, NULL, worker_function, &counter);
    
    // 互斥锁创建
    pthread_mutex_t counter_mutex;
    pthread_mutex_init(&counter_mutex, NULL);
    
    // 条件变量创建
    pthread_cond_t data_ready;
    pthread_cond_init(&data_ready, NULL);
    
    // 信号量创建
    sem_t resource_semaphore;
    sem_init(&resource_semaphore, 0, 5);
    
    // 套接字创建
    int server_socket = socket(AF_INET, SOCK_STREAM, 0);
    int client_socket = socket(AF_INET, SOCK_STREAM, 0);
    
    // 复杂组合创建
    struct ThreadContext {
        int thread_id;
        pthread_mutex_t* mutex;
        pthread_cond_t* condition;
        sem_t* semaphore;
        FILE* log_file;
        int* shared_data;
    };
    
    struct ThreadContext* contexts = malloc(sizeof(struct ThreadContext) * 5);
    for (int i = 0; i < 5; i++) {
        contexts[i].thread_id = i;
        contexts[i].mutex = &counter_mutex;
        contexts[i].condition = &data_ready;
        contexts[i].semaphore = &resource_semaphore;
        contexts[i].log_file = log_file;
        contexts[i].shared_data = dynamic_array;
        
        pthread_t thread;
        pthread_create(&thread, NULL, worker_function, &contexts[i]);
    }
}

// 复杂数据结构创建示例
void complex_data_structure_creation() {
    // 链表节点创建
    struct ListNode {
        int data;
        struct ListNode* next;
    };
    
    struct ListNode* head = malloc(sizeof(struct ListNode));
    struct ListNode* current = head;
    
    for (int i = 0; i < 10; i++) {
        current->data = i;
        if (i < 9) {
            current->next = malloc(sizeof(struct ListNode));
            current = current->next;
        } else {
            current->next = NULL;
        }
    }
    
    // 二叉树节点创建
    struct TreeNode {
        int value;
        struct TreeNode* left;
        struct TreeNode* right;
    };
    
    struct TreeNode* root = malloc(sizeof(struct TreeNode));
    root->value = 0;
    root->left = malloc(sizeof(struct TreeNode));
    root->right = malloc(sizeof(struct TreeNode));
    
    root->left->value = -1;
    root->left->left = NULL;
    root->left->right = NULL;
    
    root->right->value = 1;
    root->right->left = NULL;
    root->right->right = NULL;
    
    // 图节点创建
    struct GraphNode {
        int id;
        struct GraphNode** neighbors;
        int neighbor_count;
    };
    
    struct GraphNode* graph_nodes = malloc(sizeof(struct GraphNode) * 5);
    for (int i = 0; i < 5; i++) {
        graph_nodes[i].id = i;
        graph_nodes[i].neighbors = malloc(sizeof(struct GraphNode*) * 4);
        graph_nodes[i].neighbor_count = 0;
    }
}