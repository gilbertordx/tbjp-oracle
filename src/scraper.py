import requests
from bs4 import BeautifulSoup
import time
import json
import shutil
from pathlib import Path
from datetime import datetime
from typing import Optional

class TBJPScraper:
    def __init__(self):
        self.session = requests.Session()
        self.session.trust_env = False
        self.session.headers.update({
            'User-Agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36',
            'Accept': 'text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8',
        })
        self.base_url = "https://www.trainedbyjp.com/members/jordan-peters/forums/replies/"
        self.scraped_data = []

    @staticmethod
    def _extract_id_from_classes(classes: list[str], prefix: str) -> Optional[str]:
        for class_name in classes:
            if class_name.startswith(prefix):
                return class_name.replace(prefix, "", 1)
        return None

    @staticmethod
    def _parse_historical_timestamp(raw_date: Optional[str]) -> Optional[str]:
        if not raw_date:
            return None

        normalized = " ".join(raw_date.split())
        try:
            return datetime.strptime(normalized, "%B %d, %Y at %I:%M %p").isoformat()
        except ValueError:
            return None

    def scrape_page(self, url: str, max_retries: int = 3) -> bool:
        for attempt in range(1, max_retries + 1):
            try:
                print(f"Scraping: {url}")
                response = self.session.get(url, timeout=20)
                response.raise_for_status()

                soup = BeautifulSoup(response.text, 'html.parser')
                reply_items = soup.find_all('div', class_='bs-reply-list-item')

                if not reply_items:
                    print(f"WARNING: No replies found on this page: {url}")
                    return True

                for idx, item in enumerate(reply_items, start=1):
                    classes = item.get('class', [])
                    post_id_raw = self._extract_id_from_classes(classes, "post-")
                    thread_id_raw = self._extract_id_from_classes(classes, "bbp-parent-topic-")

                    if not post_id_raw or not thread_id_raw:
                        print(
                            f"SKIP missing IDs at {url} item {idx}: "
                            f"post_id={post_id_raw}, thread_id={thread_id_raw}"
                        )
                        continue

                    content_div = item.find('div', class_='bbp-reply-content')
                    if not content_div:
                        continue

                    signature = content_div.find('div', class_='bbp-signature')
                    if signature:
                        signature.decompose()

                    content_text = content_div.get_text(strip=True, separator='\n')
                    if not content_text:
                        continue

                    topic_link = item.find('a', class_='bbp-topic-permalink')
                    thread_title = topic_link.get_text(strip=True) if topic_link else None

                    date_elem = item.find('span', class_='bs-timestamp')
                    historical_date = date_elem.get_text(strip=True) if date_elem else None
                    timestamp_iso = self._parse_historical_timestamp(historical_date)
                    if historical_date and not timestamp_iso:
                        print(f"WARN unparseable date at {url} item {idx}: {historical_date}")

                    post = {
                        "post_id": post_id_raw,
                        "thread_id": thread_id_raw,
                        "thread_title": thread_title,
                        "author": "Jordan Peters",
                        "timestamp": timestamp_iso,
                        "date": historical_date,
                        "content": content_text,
                        "parent_post_id": None,
                        "tags": []
                    }
                    self.scraped_data.append(post)

                return True

            except requests.exceptions.RequestException as exc:
                print(f"Network error on {url} (attempt {attempt}/{max_retries}): {exc}")
                if attempt < max_retries:
                    time.sleep(3 * attempt)
                else:
                    print(f"FAILED page after retries: {url}")
                    return False
            except Exception as exc:
                print(f"Parsing error on {url} (attempt {attempt}/{max_retries}): {exc}")
                if attempt < max_retries:
                    time.sleep(2 * attempt)
                else:
                    print(f"FAILED page after retries: {url}")
                    return False

    def run(self, pages_to_scrape: int = 2):
        failed_pages: list[int] = []

        for page_number in range(1, pages_to_scrape + 1):
            if page_number == 1:
                page_url = self.base_url
            else:
                page_url = f"{self.base_url}page/{page_number}/"

            ok = self.scrape_page(page_url)
            if not ok:
                failed_pages.append(page_number)

            time.sleep(2)

        print(
            f"Scrape summary: requested={pages_to_scrape}, "
            f"failed_pages={len(failed_pages)}, extracted_posts={len(self.scraped_data)}"
        )
        if failed_pages:
            print(f"Failed page numbers: {failed_pages}")

        self._save_to_json()

    def _save_to_json(self):
        output_path = Path("data/raw_forum_data.json")
        output_path.parent.mkdir(parents=True, exist_ok=True)

        if output_path.exists():
            backup_name = (
                f"{output_path.stem}.backup-{datetime.now().strftime('%Y%m%d-%H%M%S')}"
                f"{output_path.suffix}"
            )
            backup_path = output_path.with_name(backup_name)
            shutil.copy2(output_path, backup_path)
            print(f"Backup created: {backup_path}")

        with output_path.open('w', encoding='utf-8') as f:
            json.dump(self.scraped_data, f, indent=4, ensure_ascii=False)
        print(f"Successfully saved {len(self.scraped_data)} posts to {output_path}")

if __name__ == "__main__":
    scraper = TBJPScraper()
    scraper.run(pages_to_scrape=1035)
