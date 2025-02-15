import { Priority } from "$lib/types";
import platform from "$src/platform";

import type { RepeatTask, Matter } from "$src/types";
import { v4 as uuidv4 } from "uuid";

class RepeatTaskAPI {

    public data = $state<RepeatTask[]>([]);

    public async fetchData() {
        this.data = await platform.instance.storage.listRepeatTasks();
    }

    public async createRepeatTask(task: RepeatTask) {
        const newTask = await platform.instance.storage.createRepeatTask(task);
        this.data.unshift(newTask);
    }

    public getRepeatTaskById(taskId: string): RepeatTask | undefined {
        return this.data.find((task) => task.id === taskId);
    }

    public async updateRepeatTask(task: RepeatTask) {
        let taskId = task.id;
        const newTask = await platform.instance.storage.updateRepeatTask(taskId, task);
        let index = this.data.findIndex((task) => task.id === taskId);
        if (index !== -1) {
            this.data[index] = newTask;
        }
    }

    public async deleteRepeatTask(taskId: string) {
        await platform.instance.storage.deleteRepeatTask(taskId);
        let index = this.data.findIndex((task) => task.id === taskId);
        if (index !== -1) {
            this.data.splice(index, 1);
        }
    }
    public async createMatter(repeatTask: RepeatTask) {
        let components = repeatTask.repeat_time.split("|");
        if (components.length !== 3) {
            console.error("repeat_time format error", repeatTask.repeat_time);
            return;
        }
        let now = new Date();

        let startTime = components[1]; // 08:00
        let endTime = components[2]; // 10:00

        let startTimeLocal = new Date();
        startTimeLocal.setHours(parseInt(startTime.split(":")[0]));
        startTimeLocal.setMinutes(parseInt(startTime.split(":")[1]));

        let endTimeLocal = new Date();
        endTimeLocal.setHours(parseInt(endTime.split(":")[0]));
        endTimeLocal.setMinutes(parseInt(endTime.split(":")[1]));
        let color = "";
        switch (repeatTask.priority) {
            case Priority.Low:
                color = "green";
                break;
            case Priority.Medium:
                color = "blue";
                break;
            case Priority.High:
                color = "red";
                break;
            default:
                color = "blue";
                break;
        }

        const matter: Matter = {
            id: uuidv4(),
            title: repeatTask.title,
            description: repeatTask.description || "",
            tags: repeatTask.tags || "",
            start_time: startTimeLocal.toISOString(),
            end_time: endTimeLocal.toISOString(),
            priority: repeatTask.priority,
            type_: 1, // repeat task
            created_at: now.toISOString(),
            updated_at: now.toISOString(),
            reserved_1: color,
            reserved_2: repeatTask.id,
        };

        try {
            await platform.instance.storage.createMatter(matter);
            console.log("[repeat-task] Matter created successfully");
        } catch (error) {
            console.error("[repeat-task] Failed to create matter:", error);
        }
    }
}

export const repeatTaskAPI = new RepeatTaskAPI();
