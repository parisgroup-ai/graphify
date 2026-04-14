import { pgTable, text, uuid } from 'drizzle-orm/pg-core';
import { relations } from 'drizzle-orm';

export const posts = pgTable('posts', {
  id:      uuid('id').primaryKey(),
  title:   text('title').notNull(),
  tags:    tsvector('tags').notNull(),
});

export const postsRelations = relations(posts, ({ one }) => ({
  author: one(users),
}));
