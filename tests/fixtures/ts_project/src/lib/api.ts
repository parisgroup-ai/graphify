export const api = {
    get: (path: string) => fetch(path),
    post: (path: string, body: any) => fetch(path, { method: 'POST', body }),
};

export function createClient(baseUrl: string) {
    return { baseUrl };
}
