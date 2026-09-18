using System.Windows;
using MessageBox = System.Windows.MessageBox;
using Window = System.Windows.Window;

namespace Veil.App;

public partial class MainWindow : Window
{
    private readonly PanelViewModel _vm;

    public MainWindow(PanelViewModel vm)
    {
        _vm = vm;
        DataContext = vm;
        InitializeComponent();
    }

    protected override void OnClosing(System.ComponentModel.CancelEventArgs e)
    {
        e.Cancel = true;
        Hide();
    }

    private void KeepOff_Click(object sender, RoutedEventArgs e)
    {
        if ((sender as FrameworkElement)?.DataContext is ScreenItem item)
        {
            var error = _vm.KeepOff(item);
            if (error is not null)
            {
                MessageBox.Show(error, "Veil", MessageBoxButton.OK, MessageBoxImage.Information);
            }
        }
    }

    private void Restore_Click(object sender, RoutedEventArgs e)
    {
        if ((sender as FrameworkElement)?.DataContext is ScreenItem item)
        {
            _vm.Restore(item);
        }
    }

    private void RestoreAll_Click(object sender, RoutedEventArgs e) => _vm.RestoreAll();
}
