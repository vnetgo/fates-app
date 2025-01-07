import { createTag, getAllTags, deleteTag, updateTagLastUsedAt } from "./store";
import type { Tag } from "./store";

class TagManager {
    public tagNames = $state<string[]>([]);
    public tagObjects = $state<Tag[]>([]);
    constructor() {
        this.fetchAllTags().then(() => {
            console.log("Fetch all tags success ..");
        });
    }

    public async fetchAllTags() {
        const tags = await getAllTags();
        const sortedTags = tags.sort((a: Tag, b: Tag) => {
            const dateA = new Date(a.last_used_at).getTime();
            const dateB = new Date(b.last_used_at).getTime();
            if (isNaN(dateA) || isNaN(dateB)) {
                console.warn("Invalid date format in last_used_at");
                return 0;
            }
            return dateB - dateA;
        });
        const filteredTags = sortedTags.filter((tag: Tag) => tag.name !== "");
        const newTags = filteredTags.map((tag: Tag) => tag.name);
        this.tagNames = newTags;
        this.tagObjects = filteredTags;
    }

    public async createTags(tags: string[]) {
        const filteredTags = this.processTags(tags);
        if (filteredTags.length == 0) {
            return;
        }
        const tagsStr = filteredTags.join(",");
        await createTag(tagsStr, { debug: true });
    }

    public async deleteTags(tags: string[]) {
        const filteredTags = this.processTags(tags);
        if (filteredTags.length == 0) {
            return;
        }
        const tagsStr = filteredTags.join(",");
        await deleteTag(tagsStr, { debug: true });
    }

    public async updateTagsLastUsedAt(tags: string[]) {
        const filteredTags = this.processTags(tags);
        if (filteredTags.length == 0) {
            return;
        }
        const tagsStr = filteredTags.join(",");
        await updateTagLastUsedAt(tagsStr, { debug: true });
    }

    private  processTags(tags: string[]) {
        return [
            ...new Set(
                tags
                    .filter((tag) => typeof tag === "string")
                    .map((tag) => tag.trim())
                    .filter((tag) => tag.length > 0)
            ),
        ];
    }
}
export const tagManager = new TagManager();

export default tagManager;
