-- Allow the native macOS first-install artifact in the private canary bucket.
-- Existing ZIP and manifest MIME types remain unchanged.

do $$
begin
  if to_regclass('storage.buckets') is not null then
    update storage.buckets
    set allowed_mime_types = case
      when 'application/x-apple-diskimage' = any(coalesce(allowed_mime_types, array[]::text[]))
        then allowed_mime_types
      else array_append(
        coalesce(allowed_mime_types, array[]::text[]),
        'application/x-apple-diskimage'
      )
    end
    where id = 'mdx-canary-releases';
  end if;
end
$$;
