#!/usr/bin/env bash
# Run only from trusted base-branch code. Contributor text stays in JSON.
set -euo pipefail
repo=${GITHUB_REPOSITORY:?}
[[ ${PR:?} =~ ^[1-9][0-9]*$ ]]
pr=$(gh api "repos/$repo/pulls/$PR")
jq -e '.state == "open" and .base.ref == "main"' <<<"$pr" >/dev/null || exit 0
head=$(jq -er '.head.sha' <<<"$pr")
url="$GITHUB_SERVER_URL/$repo/actions/runs/$GITHUB_RUN_ID"
status() {
    gh api -X POST "repos/$repo/statuses/$head" -f context='IC Wire CLA' \
        -f state="$1" -f description="$2" -f target_url="$url" >/dev/null
}
status pending 'Checking contributor agreements'
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
query=$(cat <<'GRAPHQL'
query($owner:String!, $name:String!, $number:Int!, $endCursor:String) {
  repository(owner:$owner,name:$name) { pullRequest(number:$number) {
    headRefOid commits(first:100,after:$endCursor) {
      totalCount pageInfo { hasNextPage endCursor }
      nodes { commit { authors(first:100) { totalCount nodes { user { databaseId } } } } }
    }
  } }
}
GRAPHQL
)
gh api graphql --paginate --slurp -f query="$query" -f owner="${repo%/*}" \
    -f name="${repo#*/}" -F number="$PR" > "$scratch/commits"
gh api --paginate --slurp "repos/$repo/issues/$PR/comments?per_page=100" > "$scratch/comments"
statement='I agree to the IC Wire Contributor Licence Agreement v1 for my contributions to this pull request.'
if jq -en --argjson pr "$pr" --arg statement "$statement" \
    --slurpfile pages "$scratch/commits" --slurpfile comments "$scratch/comments" '
    [$pages[0][] | .data.repository.pullRequest] as $pulls |
    [$pulls[].commits.nodes[].commit.authors] as $authors |
    [$pr.user.id, ($authors[].nodes[].user.databaseId)] | unique as $required |
    [$comments[0][][] | select(.user.type == "User" and .body == $statement and .created_at == .updated_at) | .user.id] as $signed |
    ($pulls | length > 0) and all($pulls[]; .headRefOid == $pr.head.sha and .commits.totalCount == $pr.commits)
    and ($authors | length == $pr.commits and length > 0)
    and all($authors[]; .totalCount > 0 and .totalCount == (.nodes | length))
    and all($required[]; type == "number") and (($required - $signed) | length == 0)
' >/dev/null; then
    status success 'All identified contributors accepted the CLA'
else
    status failure 'Missing agreement or attribution. See CONTRIBUTING.md'
    exit 1
fi
