<script lang="ts">
    import { Badge } from "$lib/components/ui/badge";
    import { Button } from "$lib/components/ui/button";
    import { Input } from "$lib/components/ui/input";
    import { Label } from "$lib/components/ui/label";
    import * as Table from "$lib/components/ui/table/index";
    import { v4 as uuidv4 } from "uuid";
    import dayjs from "dayjs";

    import AlertDialog from "$lib/components/alert-dialog.svelte";
    import platform from "$src/platform";
    import { TableHandler } from "@vincjo/datatables";
    import { ChevronLeft, ChevronRight, Trash2 } from "lucide-svelte";
    import { onMount } from "svelte";
    import { t } from "svelte-i18n";
    import type { Matter, Todo } from "$src/types";
    import DataTableTextInputCell from "./data-table-text-input-cell.svelte";
    import { REFRESH_TIME_PROGRESS } from "$src/config";
    import DataTableSelectedTimeCell from "./data-table-selected-time-cell.svelte";

    let alertOpen = $state(false);
    let alertTitle = $state("");
    let alertContent = $state("");
    let alertShowCancel = $state(false);
    let alertConfirm: () => Promise<void> = $state(async () => {});
    class TodoAPI {
        public data = $state<Todo[]>([]);
        private matters: Matter[] = [];

        async syncTodoStatus() {
            console.log("[TodoPage] Sync todo status ...");
            //  get matters with type_ 2

            const matters = await platform.instance.storage.queryMattersByField("type_", "2", false);
            console.log("[TodoPage] Matters: ", matters);

            this.matters = matters;
            const now = new Date();

            // get all todos
            const todos = await platform.instance.storage.listTodos();
            const getTodoById = (id: string) => todos.find((item) => item.id === id);

            for (const todo of todos) {
                if (!this.matters.some((matter) => matter.reserved_2 === todo.id)) {
                    console.log("[TodoPage] Update todo status: [", todo.id, "] to [todo]");
                    await platform.instance.storage.updateTodo(todo.id, { ...todo, status: "todo" });
                }
            }

            for (const matter of matters) {
                const todoId = matter.reserved_2;
                if (!todoId) {
                    console.log("[TodoPage] Matter [", matter.id, "] has no todoId");
                    continue;
                }

                const todo = getTodoById(todoId);
                if (!todo) {
                    console.log("[TodoPage] Matter [", matter.id, "] has no todo");
                    continue;
                }

                const startTime = new Date(matter.start_time);
                const endTime = new Date(matter.end_time);

                let newStatus = todo.status;
                if (!matter.sub_type || matter.sub_type === 0) {
                    if (now < startTime) {
                        newStatus = "todo";
                    } else if (now >= startTime && now <= endTime) {
                        newStatus = "in_progress";
                    } else if (now > endTime) {
                        newStatus = "completed";
                    }
                } else if (matter.sub_type === 1) {
                    newStatus = "completed";
                }

                console.log("[TodoPage] Update todo status: [", todo.id, "] to [", newStatus, "]");
                await platform.instance.storage.updateTodo(todoId, {
                    ...todo,
                    status: newStatus,
                    start_time: dayjs(startTime).format("YYYY-MM-DDTHH:mm"),
                    updated_at: now.toISOString(),
                });
            }
        }

        async fetchData() {
            await this.syncTodoStatus();
            let allTodos = await platform.instance.storage.listTodos();
            // sort by created_at
            // 分组并排序
            const sortedTodos = ["todo", "in_progress", "completed"]
                .map((status) =>
                    allTodos
                        .filter((todo) => todo.status === status)
                        .sort((a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime())
                )
                .flat();
            this.data = sortedTodos;
        }

        async createTodo(todo: Todo) {
            console.log("createTodo: ", todo);
            await platform.instance.storage.createTodo(todo);
            await this.fetchData();
        }

        async deleteTodo(id: string) {
            alertTitle = $t("app.other.confirmDelete");
            alertContent = $t("app.other.confirmDeleteDescription");
            alertShowCancel = true;
            alertConfirm = async () => {
                console.log("deleteTodo: ", id);
                await platform.instance.storage.deleteTodo(id);
                await this.fetchData();
            };
            alertOpen = true;
        }

        async updateTodo(todo: Todo) {
            await platform.instance.storage.updateTodo(todo.id, todo);
            await this.fetchData();
        }

        getTodoById(id: string) {
            return this.data.find((item) => item.id === id);
        }

        isTodoInProgress(todoId: string): boolean {
            return this.matters.some((matter) => matter.reserved_2 === todoId);
        }
    }

    const todoAPI = new TodoAPI();
    let table = new TableHandler(todoAPI.data, { rowsPerPage: 10 });
    const search = table.createSearch();

    $effect(() => {
        todoAPI.data;
        table.setRows(todoAPI.data);
    });

    const handleDelete = async (rowId: string) => {
        console.log("handleDelete: ", rowId);
        await todoAPI.deleteTodo(rowId);
    };

    const handleCreate = async () => {
        let now = dayjs().format("HHmmss");
        const defaultTodo = {
            id: uuidv4(),
            title: `#${$t("app.todo.defaultTodoTitle")}${now}`,
            status: "todo",
            created_at: new Date().toISOString(),
            updated_at: new Date().toISOString(),
        };
        console.log("createTodo: ", defaultTodo);
        await todoAPI.createTodo(defaultTodo);
    };

    async function onUpdateValue(rowId: string, columnId: string, value: string) {
        let todo = todoAPI.getTodoById(rowId);
        if (!todo) {
            console.error("todo not found", rowId);
            return;
        }
        console.log("onUpdateValue: ", todo);
        if (columnId === "title") {
            await todoAPI.updateTodo({ ...todo, title: value });
        } else if (columnId === "start_time") {
            console.log("[TodoPage] Update start_time: ", value);
            await todoAPI.updateTodo({ ...todo, start_time: value });
        }
    }

    // matterType: 2 = todo range item, 3 = todo item
    const handleExecute = async (row: Todo, isPointItem: boolean) => {
        let start_time = row.start_time;

        if (!start_time) {
            start_time = dayjs().format("YYYY-MM-DDTHH:mm");
        }

        let start_time_date = dayjs(start_time);

        if (!start_time_date.isValid()) {
            start_time_date = dayjs();
        }

        let end_time_data = undefined;

        if (!isPointItem) {
            end_time_data = start_time_date.add(2, "hour");
        }

        const matter: Matter = {
            id: uuidv4(),
            title: row.title,
            type_: 2,
            sub_type: isPointItem ? 1 : 0,
            start_time: start_time_date.toISOString(),
            end_time: end_time_data?.toISOString() || "",
            priority: 0,
            created_at: new Date().toISOString(),
            updated_at: new Date().toISOString(),
            reserved_1: "blue",
            reserved_2: row.id,
        };

        await platform.instance.storage.createMatter(matter);

        if (!isPointItem) {
            let now = new Date();
            const startTime = new Date(matter.start_time);
            const endTime = new Date(matter.end_time);

            let newStatus = "todo";
            if (!matter.sub_type || matter.sub_type === 0) {
                if (now < startTime) {
                    newStatus = "todo";
                } else if (now >= startTime && now <= endTime) {
                    newStatus = "in_progress";
                } else if (now > endTime) {
                    newStatus = "completed";
                }
            } else if (matter.sub_type === 1) {
                newStatus = "completed";
            }
            console.log("[TodoPage] Update todo status: [", row.id, "] to [", newStatus, "]");
            await todoAPI.updateTodo({ ...row, status: newStatus });
        } else {
            await todoAPI.updateTodo({ ...row, status: "completed" });
        }

        await platform.instance.event.emit(REFRESH_TIME_PROGRESS, {});

        if (!isPointItem) {
            alertTitle = $t("app.other.tip");
            alertContent = `「${row.title}」${$t("app.todo.todoInProgressDescription")}`;
            alertConfirm = async () => {};
            alertShowCancel = false;
            alertOpen = true;
        }
    };

    onMount(() => {
        todoAPI
            .fetchData()
            .then(() => {
                console.log("[TodoPage] Fetch all todos successfully");
            })
            .catch((error) => {
                console.error("[TodoPage] Fetch all todos error: ", error);
            });
    });
</script>

<div class="flex flex-col h-full">
    <div class="flex flex-col px-6 pt-6 gap-4">
        <Label class="text-2xl font-bold tracking-tight">{$t("app.todo.title")}</Label>
        <Label class="text-base text-muted-foreground">{$t("app.todo.description")}</Label>
    </div>
    <div class="flex flex-col flex-1 p-6 gap-2">
        <div class="flex items-center justify-between">
            <div class="flex flex-1 items-center space-x-2">
                <Input
                    placeholder={$t("app.todo.searchPlaceholder")}
                    class="bg-background h-8 w-[150px] lg:w-[250px]"
                    type="search"
                    disabled={table.rows.length === 0}
                    bind:value={search.value}
                    oninput={() => {
                        search.set();
                    }}
                />
            </div>
            <div class="flex items-center space-x-2">
                <Button
                    onclick={() => {
                        handleCreate();
                    }}
                    class="text-primary-foreground"
                >
                    {$t("app.todo.createTodo")}
                </Button>
            </div>
        </div>
        <div class="rounded-md border">
            <Table.Root>
                <Table.Header>
                    <Table.Row>
                        <Table.Head>{$t("app.todo.name")}</Table.Head>
                        <Table.Head>{$t("app.todo.start_time")}</Table.Head>
                        <Table.Head>{$t("app.todo.status")}</Table.Head>
                        <Table.Head>{$t("app.todo.action")}</Table.Head>
                    </Table.Row>
                </Table.Header>
                <Table.Body>
                    {#if table.rows.length > 0}
                        {#each table.rows as row (row.id)}
                            <Table.Row>
                                <Table.Cell>
                                    <DataTableTextInputCell
                                        rowId={row.id}
                                        value={row.title}
                                        disabled={todoAPI.isTodoInProgress(row.id) || row.status === "completed"}
                                        onUpdateValue={(rowId, newValue) => {
                                            console.log("[TodoPage] Update title: ", rowId, newValue);
                                            onUpdateValue(rowId, "title", newValue);
                                        }}
                                    />
                                </Table.Cell>

                                <Table.Cell class="w-[208px]">
                                    <DataTableSelectedTimeCell
                                        rowId={row.id}
                                        selectedTime={row.start_time || ""}
                                        disabled={todoAPI.isTodoInProgress(row.id) || row.status === "completed"}
                                        onUpdateValue={(rowId, newValue) => {
                                            onUpdateValue(rowId, "start_time", newValue);
                                        }}
                                    />
                                </Table.Cell>

                                <Table.Cell class="w-[96px]">
                                    <Badge
                                        variant={row.status === "completed"
                                            ? "default"
                                            : row.status === "in_progress"
                                              ? "secondary"
                                              : "outline"}
                                    >
                                        {row.status === "todo"
                                            ? $t("app.todo.statusOptions.todo")
                                            : row.status === "in_progress"
                                              ? $t("app.todo.statusOptions.in_progress")
                                              : $t("app.todo.statusOptions.completed")}
                                    </Badge>
                                </Table.Cell>

                                <Table.Cell class="w-[208px]">
                                    <div class="flex gap-2">
                                        <Button variant="outline" size="sm" onclick={() => handleDelete(row.id)}>
                                            <Trash2 />
                                        </Button>
                                        {#if row.status === "todo"}
                                            {#if todoAPI.isTodoInProgress(row.id)}
                                                <Button disabled variant="outline" size="sm">
                                                    <!-- 已添加 -->
                                                    {$t("app.todo.added")}
                                                </Button>
                                            {:else}
                                                <Button
                                                    variant="outline"
                                                    size="sm"
                                                    onclick={() => handleExecute(row, false)}
                                                >
                                                    {$t("app.todo.todoInProgress")}
                                                </Button>
                                                <Button
                                                    variant="outline"
                                                    size="sm"
                                                    onclick={() => handleExecute(row, true)}
                                                >
                                                    {$t("app.todo.todoCompleted")}
                                                </Button>
                                            {/if}
                                        {/if}
                                    </div>
                                </Table.Cell>
                            </Table.Row>
                        {/each}
                    {/if}
                </Table.Body>
            </Table.Root>
        </div>
        <div class="flex flex-row justify-between">
            <div class="flex flex-col">
                <!-- total todo count -->
                <Label class="text-sm text-muted-foreground">
                    {$t("app.todo.totalTodo")}: {table.rows.length}
                </Label>
            </div>
            <div class="flex justify-end items-center space-x-2">
                <Label class="text-sm text-muted-foreground">
                    {$t("app.other.page0")}
                    {table.currentPage}
                    {$t("app.other.page1")}
                    {Math.max(table.pageCount, 1)}
                    {$t("app.other.page2")}
                </Label>
                <Button
                    class="w-8 h-8"
                    disabled={table.currentPage === 1}
                    variant="outline"
                    size="icon"
                    onclick={() => table.setPage("previous")}
                >
                    <ChevronLeft />
                </Button>
                <Button
                    class="w-8 h-8"
                    disabled={table.currentPage === table.pageCount}
                    variant="outline"
                    size="icon"
                    onclick={() => table.setPage("next")}
                >
                    <ChevronRight />
                </Button>
            </div>
        </div>
    </div>
</div>

<AlertDialog
    bind:open={alertOpen}
    title={alertTitle}
    content={alertContent}
    onConfirm={alertConfirm}
    showCancel={alertShowCancel}
/>
