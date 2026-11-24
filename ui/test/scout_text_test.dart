import 'package:elsewhen_ui/providers/scout_research_provider.dart';
import 'package:elsewhen_ui/utils/scout_text.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  const intro = 'Ready to ship? One agent tests, translates and audits it';
  const encoded =
      '&lt;p&gt;$intro&lt;/p&gt; &lt;p&gt; &lt;a href=&quot;https://www.producthunt.com/products/kadeep-ai?utm_campaign=producthunt-atom-posts-feed&amp;amp;utm_medium=rss&quot;&gt;Discussion&lt;/a&gt; | &lt;a href=&quot;https://www.producthunt.com/r/p/1319952?app_id=339&quot;&gt;Link&lt;/a&gt; &lt;/p&gt;';
  test('escaped Product Hunt markup becomes only its introduction', () {
    expect(cleanScoutText(encoded, feedNavigation: true), intro);
    expect(
      cleanScoutText(encoded.replaceAll('&', '&amp;'), feedNavigation: true),
      intro,
    );
  });
  test(
    'HTML entities, paragraphs and unsafe or invisible blocks are handled',
    () {
      expect(
        cleanScoutText(
          '<p>Client&nbsp;&amp;&nbsp;work</p><p>&#x1F680; &#169;</p><script>secret()</script><style>bad</style>',
        ),
        'Client & work 🚀 ©',
      );
      expect(cleanScoutText('Cost < 10 & save > 2'), 'Cost < 10 & save > 2');
      expect(
        cleanScoutText(
          'Keep <a href="/help">help text</a>',
          feedNavigation: true,
        ),
        'Keep help text',
      );
    },
  );
  test('description falls through empty or markup-only values without changing evidence', () {
    final payload = {
      'description': '<p> </p>',
      'summary': encoded,
      'artistName': 'Microsoft &amp; Co',
      'genres_0_name': 'Productivity',
    };
    final product = ScoutProduct({
      'title': 'KaDeep &amp; AI',
      'payload': payload,
    });
    expect(product.title, 'KaDeep & AI');
    expect(product.description, intro);
    expect(product.category, 'Productivity');
    expect(product.maker, 'Microsoft & Co');
    expect(payload['summary'], encoded);
    expect(ScoutProduct({'payload': {}}).description, isNull);
  });
}
