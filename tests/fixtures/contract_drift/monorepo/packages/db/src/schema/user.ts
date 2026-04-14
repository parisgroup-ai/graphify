import { pgTable, text, integer, uuid, timestamp } from 'drizzle-orm/pg-core';

export const users = pgTable('users', {
  id:        uuid('id').primaryKey(),
  email:     text('email').notNull(),
  age:       integer('age').notNull(),
  createdAt: timestamp('created_at').notNull(),
});
