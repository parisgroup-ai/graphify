import { api } from '@/lib/api';

export class UserService {
    async getUser(id: string) {
        return api.get(`/users/${id}`);
    }

    async createUser(name: string) {
        return api.post('/users', { name });
    }
}
