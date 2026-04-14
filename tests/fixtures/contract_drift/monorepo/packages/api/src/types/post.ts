export interface PostDto {
  id: string;
  title: string;
  tags: string;
  authors: PostDto[];
}
