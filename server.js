import { serve } from "bun";
import { join } from "path";

serve({
    port: 3000,
    async fetch(req) {
        const url = new URL(req.url);
        let filePath = join("./dist", url.pathname);

        // If the request is for a directory, serve index.html inside it
        try {
            const file = Bun.file(filePath);
            if (await file.exists()) {
                return new Response(file);
            }
        } catch {
            // ignore and fallback
        }

        // Fallback: serve ./dist/index.html (SPA routing)
        return new Response(Bun.file("./dist/index.html"));
    },
});
